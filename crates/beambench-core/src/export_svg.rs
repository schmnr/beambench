//! SVG export for projects.

use crate::object::{ObjectData, ObjectId};
use crate::project::Project;
use crate::vector::convert::object_to_world_vecpath;

/// Export project as SVG XML.
pub fn export_svg(project: &Project, selection_only: bool, selected_ids: &[ObjectId]) -> String {
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

    svg.push_str("</svg>\n");
    svg
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{ObjectData, ProjectObject};
    use beambench_common::{Bounds, Point2D};

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
        let svg = export_svg(&project, false, &[]);
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
        let svg = export_svg(&project, false, &[]);
        assert!(svg.contains("<path"));
        assert!(svg.contains("d="));
    }

    #[test]
    fn export_svg_selection_only() {
        let (project, obj_id) = test_project();

        let svg_all = export_svg(&project, false, &[]);
        let svg_selected = export_svg(&project, true, &[obj_id]);
        let svg_empty = export_svg(&project, true, &[]);

        assert!(svg_all.contains("<path"));
        assert!(svg_selected.contains("<path"));
        assert!(!svg_empty.contains("<path"));
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
        let svg = export_svg(&project, false, &[]);
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
        let svg = export_svg(&project, false, &[]);
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
        let svg = export_svg(&project, false, &[]);
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
        let svg = export_svg(&project, false, &[]);
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
        let svg = export_svg(&project, false, &[]);
        assert!(
            !svg.contains("<text"),
            "rtl text should not emit editable <text> that duplicates geometry"
        );
        assert!(svg.contains("<path"), "rtl text should export as path");
        assert_eq!(svg.matches("<path").count(), 1);
    }
}

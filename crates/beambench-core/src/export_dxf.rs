//! DXF export for projects.

use crate::object::ObjectId;
use crate::project::Project;
use crate::vector::convert::object_to_world_vecpath;
use beambench_common::path::PathCommand;

/// Export project as DXF text.
pub fn export_dxf(project: &Project, selection_only: bool, selected_ids: &[ObjectId]) -> String {
    let mut dxf = String::new();

    // Header
    dxf.push_str("0\nSECTION\n2\nHEADER\n");
    dxf.push_str("9\n$ACADVER\n1\nAC1015\n"); // AutoCAD 2000
    dxf.push_str("9\n$INSUNITS\n70\n4\n"); // 4 = millimeters
    dxf.push_str("0\nENDSEC\n");

    // Entities section
    dxf.push_str("0\nSECTION\n2\nENTITIES\n");

    // Pre-process: expand VirtualClone objects for export
    let expanded_clones: Vec<_> = project
        .objects
        .iter()
        .filter_map(|obj| project.resolve_clone(obj))
        .collect();
    let all_objects: Vec<&crate::ProjectObject> = project
        .objects
        .iter()
        .filter(|o| !matches!(o.data, crate::ObjectData::VirtualClone { .. }))
        .chain(expanded_clones.iter())
        .collect();

    for obj in all_objects {
        if selection_only && !selected_ids.contains(&obj.id) {
            continue;
        }

        if !obj.visible {
            continue;
        }

        // Get layer name for DXF entity
        let layer_name = project
            .layers
            .iter()
            .find(|l| l.id == obj.layer_id)
            .map(|l| l.name.as_str())
            .unwrap_or("0");

        // Text exports as line geometry only. Emitting MTEXT plus line fallback
        // duplicates laser/CAD geometry and can place the editable text at a
        // different anchor than the resolved glyph outlines.
        if let Some(path) = object_to_world_vecpath(obj) {
            // Flatten path into line segments
            for subpath in &path.subpaths {
                let mut last_point: Option<(f64, f64)> = None;

                for cmd in &subpath.commands {
                    match cmd {
                        PathCommand::MoveTo { x, y } => {
                            last_point = Some((*x, *y));
                        }
                        PathCommand::LineTo { x, y } => {
                            if let Some((x1, y1)) = last_point {
                                let x2 = *x;
                                let y2 = *y;
                                dxf.push_str(&format!(
                                    "0\nLINE\n8\n{}\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n",
                                    layer_name, x1, y1, x2, y2
                                ));
                                last_point = Some((x2, y2));
                            }
                        }
                        PathCommand::QuadTo { x, y, .. } | PathCommand::CubicTo { x, y, .. } => {
                            // Approximate curves as straight lines to endpoint
                            if let Some((x1, y1)) = last_point {
                                let x2 = *x;
                                let y2 = *y;
                                dxf.push_str(&format!(
                                    "0\nLINE\n8\n{}\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n",
                                    layer_name, x1, y1, x2, y2
                                ));
                                last_point = Some((x2, y2));
                            }
                        }
                        PathCommand::Close => {
                            // Close path back to first point (stored separately if needed)
                        }
                    }
                }
            }
        }
    }

    dxf.push_str("0\nENDSEC\n");
    dxf.push_str("0\nEOF\n");
    dxf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{ObjectData, ProjectObject};
    use beambench_common::{Bounds, Point2D};

    fn test_project() -> Project {
        let mut project = Project::new("DXF Export Test");
        let layer_id = project.ensure_default_layer();

        let obj = ProjectObject::new(
            "rect",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
            ObjectData::Shape {
                kind: crate::object::ShapeKind::Rectangle,
                width: 100.0,
                height: 100.0,
                corner_radius: 0.0,
            },
        );
        project.add_object(obj);
        project
    }

    #[test]
    fn export_dxf_includes_header() {
        let project = test_project();
        let dxf = export_dxf(&project, false, &[]);
        assert!(dxf.contains("SECTION"));
        assert!(dxf.contains("HEADER"));
        assert!(dxf.contains("ENTITIES"));
        assert!(dxf.contains("$INSUNITS"), "expected $INSUNITS header");
        assert!(dxf.contains("\n4\n"), "expected millimeter unit code (4)");
    }

    #[test]
    fn export_dxf_includes_line_entities() {
        let project = test_project();
        let dxf = export_dxf(&project, false, &[]);
        assert!(dxf.contains("LINE"));
    }

    #[test]
    fn export_dxf_ends_with_eof() {
        let project = test_project();
        let dxf = export_dxf(&project, false, &[]);
        assert!(dxf.contains("EOF"));
    }

    fn text_project_with_system_font() -> Project {
        use crate::object::{TextAlignment, TextAlignmentV, TextFontSource, TextLayoutMode};
        let mut project = Project::new("DXF Text Test");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "label",
            layer_id,
            Bounds::new(Point2D::new(5.0, 5.0), Point2D::new(50.0, 15.0)),
            ObjectData::Text {
                content: "Hello DXF".to_string(),
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

    #[test]
    fn export_dxf_text_exports_lines_only() {
        let project = text_project_with_system_font();
        let dxf = export_dxf(&project, false, &[]);
        assert!(
            !dxf.contains("MTEXT"),
            "DXF should not emit MTEXT that duplicates line geometry"
        );
        assert!(
            !dxf.contains("Hello DXF"),
            "DXF should not emit editable text content separately"
        );
        assert!(!dxf.contains("Arial"), "DXF should not emit text font");
        assert!(
            dxf.contains("LINE"),
            "DXF should contain text line geometry"
        );
    }

    #[test]
    fn export_dxf_path_text_no_mtext() {
        use crate::object::{TextAlignment, TextAlignmentV, TextFontSource, TextLayoutMode};
        let mut project = Project::new("Path Text DXF");
        let layer_id = project.ensure_default_layer();
        // Modern path-text
        project.add_object(ProjectObject::new(
            "path_text",
            layer_id,
            Bounds::new(Point2D::new(5.0, 5.0), Point2D::new(50.0, 15.0)),
            ObjectData::Text {
                content: "Curved DXF".to_string(),
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
            "legacy",
            layer_id,
            Bounds::new(Point2D::new(5.0, 20.0), Point2D::new(50.0, 30.0)),
            ObjectData::Text {
                content: "Legacy DXF".to_string(),
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
        let dxf = export_dxf(&project, false, &[]);
        assert!(
            !dxf.contains("MTEXT"),
            "DXF should NOT contain MTEXT for path-text objects"
        );
        assert!(
            dxf.contains("LINE"),
            "DXF should contain LINE entities from path fallback"
        );
    }

    #[test]
    fn export_dxf_text_missing_font_no_mtext() {
        use crate::object::{TextAlignment, TextAlignmentV, TextLayoutMode};
        let mut project = Project::new("Missing Font DXF");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "label",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(50.0, 10.0)),
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
        let dxf = export_dxf(&project, false, &[]);
        assert!(
            !dxf.contains("MTEXT"),
            "DXF should NOT contain MTEXT for missing font"
        );
        assert!(
            dxf.contains("LINE"),
            "DXF should contain LINE from path fallback"
        );
    }

    #[test]
    fn export_dxf_styled_text_exports_lines_only() {
        use crate::object::{TextAlignment, TextAlignmentV, TextFontSource, TextLayoutMode};
        let mut project = Project::new("Styled DXF");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "styled",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(80.0, 10.0)),
            ObjectData::Text {
                content: "hello".to_string(),
                font_family: "Arial".to_string(),
                font_size_mm: 8.0,
                alignment: TextAlignment::Right,
                alignment_v: TextAlignmentV::Top,
                bold: false,
                italic: false,
                upper_case: true,
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
        let dxf = export_dxf(&project, false, &[]);
        assert!(
            !dxf.contains("MTEXT"),
            "styled text should not emit MTEXT that duplicates line geometry"
        );
        assert!(
            !dxf.contains("HELLO"),
            "styled text should not emit editable text content separately"
        );
        assert!(dxf.contains("LINE"), "styled text should export as lines");
    }

    #[test]
    fn export_dxf_centered_text_exports_lines_only() {
        use crate::object::{TextAlignment, TextAlignmentV, TextFontSource, TextLayoutMode};
        let mut project = Project::new("VAlign DXF");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "middle_center",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(50.0, 20.0)),
            ObjectData::Text {
                content: "Center".to_string(),
                font_family: "Arial".to_string(),
                font_size_mm: 8.0,
                alignment: TextAlignment::Center,
                alignment_v: TextAlignmentV::Middle,
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
        let dxf = export_dxf(&project, false, &[]);
        assert!(
            !dxf.contains("MTEXT"),
            "centered text should not emit MTEXT that can anchor differently"
        );
        assert!(dxf.contains("LINE"), "centered text should export as lines");
    }
}

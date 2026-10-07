//! EPS and AI export for projects.

use crate::export_bitmap::processed_bitmap_image_for_object;
use crate::export_common::{
    POINTS_PER_MM, canvas_to_y_up, exportable_objects, matrix_operands,
    raster_unit_square_to_canvas,
};
use crate::object::{ObjectData, ObjectId};
use crate::project::Project;
use crate::vector::convert::object_to_world_vecpath;
use beambench_common::path::PathCommand;
use beambench_common::{Point2D, Transform2D};

/// Header variant for EPS vs AI export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpsHeaderVariant {
    /// Standard EPS header.
    Eps,
    /// Adobe Illustrator compatibility header.
    Ai,
}

/// Export project as EPS PostScript.
pub fn export_eps(
    project: &Project,
    selection_only: bool,
    selected_ids: &[ObjectId],
) -> Result<String, String> {
    export_ps(project, selection_only, selected_ids, EpsHeaderVariant::Eps)
}

/// Export project as AI (Adobe Illustrator) PostScript.
pub fn export_ai(
    project: &Project,
    selection_only: bool,
    selected_ids: &[ObjectId],
) -> Result<String, String> {
    export_ps(project, selection_only, selected_ids, EpsHeaderVariant::Ai)
}

fn export_ps(
    project: &Project,
    selection_only: bool,
    selected_ids: &[ObjectId],
    variant: EpsHeaderVariant,
) -> Result<String, String> {
    let w = project.workspace.bed_width_mm;
    let h = project.workspace.bed_height_mm;
    // Convert mm to PostScript points (1 pt = 25.4/72 mm)
    let w_pt = (w / 25.4 * 72.0).round() as i64;
    let h_pt = (h / 25.4 * 72.0).round() as i64;

    let mut ps = String::new();

    // Header
    match variant {
        EpsHeaderVariant::Eps => {
            ps.push_str("%!PS-Adobe-3.0 EPSF-3.0\n");
        }
        EpsHeaderVariant::Ai => {
            ps.push_str("%!PS-Adobe-3.0\n");
            ps.push_str("%%Creator: Adobe Illustrator(TM) 3.0\n");
            ps.push_str("%%AI5_FileFormat 3\n");
            ps.push_str(&format!("%%Title: ({})\n", project.metadata.project_name));
        }
    }
    ps.push_str(&format!("%%BoundingBox: 0 0 {} {}\n", w_pt, h_pt));
    ps.push_str("%%EndComments\n");

    let to_page = canvas_to_y_up(h, POINTS_PER_MM);
    let point = |x: f64, y: f64| {
        let p = to_page.apply(&Point2D::new(x, y));
        format!("{:.4} {:.4}", p.x, p.y)
    };

    // Body: visible objects as stroked paths, images as grayscale bitmaps
    for obj in exportable_objects(project, selection_only, selected_ids) {
        if matches!(obj.data, ObjectData::RasterImage { .. }) {
            let image = processed_bitmap_image_for_object(project, &obj)?;
            let placement = to_page.compose(&raster_unit_square_to_canvas(&obj));
            push_ps_gray_image(&mut ps, &image, &placement);
            continue;
        }
        // Text exports as geometry only. Emitting PostScript text operators
        // plus path fallback duplicates geometry and loses Beam Bench layout
        // fidelity for alignment, spacing, welded, and distorted text.
        if let Some(mut path) = object_to_world_vecpath(&obj) {
            crate::vector::flatten::convert_quadratics_to_cubics(&mut path);
            for subpath in &path.subpaths {
                for cmd in &subpath.commands {
                    match cmd {
                        PathCommand::MoveTo { x, y } => {
                            ps.push_str(&format!("{} moveto\n", point(*x, *y)));
                        }
                        PathCommand::LineTo { x, y } => {
                            ps.push_str(&format!("{} lineto\n", point(*x, *y)));
                        }
                        PathCommand::CubicTo {
                            c1x,
                            c1y,
                            c2x,
                            c2y,
                            x,
                            y,
                        } => {
                            ps.push_str(&format!(
                                "{} {} {} curveto\n",
                                point(*c1x, *c1y),
                                point(*c2x, *c2y),
                                point(*x, *y)
                            ));
                        }
                        PathCommand::QuadTo { .. } => {
                            unreachable!("quadratics were converted above")
                        }
                        PathCommand::Close => {
                            ps.push_str("closepath\n");
                        }
                    }
                }
                ps.push_str("stroke\n");
            }
        }
    }

    // Footer
    ps.push_str("showpage\n");
    ps.push_str("%%EOF\n");
    Ok(ps)
}

/// Draw an 8-bit grayscale image with its unit square mapped by `placement`.
fn push_ps_gray_image(ps: &mut String, image: &image::GrayImage, placement: &Transform2D) {
    let (width, height) = image.dimensions();
    ps.push_str("gsave\n");
    ps.push_str(&format!("[{}] concat\n", matrix_operands(placement)));
    ps.push_str(&format!("/picstr {width} string def\n"));
    ps.push_str(&format!(
        "{width} {height} 8 [{width} 0 0 -{height} 0 {height}] {{currentfile picstr readhexstring pop}} image\n"
    ));
    for row in image.as_raw().chunks(40) {
        for byte in row {
            ps.push_str(&format!("{byte:02X}"));
        }
        ps.push('\n');
    }
    ps.push_str("grestore\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{ObjectData, ProjectObject, ShapeKind};
    use beambench_common::{Bounds, Point2D};

    fn test_project() -> Project {
        let mut project = Project::new("EPS Test");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "rect",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(60.0, 60.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 50.0,
                height: 50.0,
                corner_radius: 0.0,
            },
        ));
        project
    }

    #[test]
    fn export_eps_includes_header_and_footer() {
        let project = test_project();
        let eps = export_eps(&project, false, &[]).unwrap();
        assert!(eps.contains("%!PS-Adobe-3.0 EPSF-3.0"));
        assert!(eps.contains("%%BoundingBox:"));
        assert!(eps.contains("%%EndComments"));
        assert!(eps.contains("showpage"));
        assert!(eps.contains("%%EOF"));
    }

    #[test]
    fn export_eps_contains_ps_commands() {
        let project = test_project();
        let eps = export_eps(&project, false, &[]).unwrap();
        assert!(eps.contains("moveto"));
        assert!(eps.contains("lineto"));
        assert!(eps.contains("stroke"));
    }

    #[test]
    fn export_eps_empty_project() {
        let project = Project::new("Empty");
        let eps = export_eps(&project, false, &[]).unwrap();
        assert!(eps.contains("%!PS-Adobe-3.0 EPSF-3.0"));
        assert!(eps.contains("%%EOF"));
        assert!(!eps.contains("moveto"));
    }

    #[test]
    fn export_ai_includes_creator_header() {
        let project = test_project();
        let ai = export_ai(&project, false, &[]).unwrap();
        assert!(ai.contains("%%Creator: Adobe Illustrator"));
        assert!(ai.contains("%%AI5_FileFormat 3"));
        assert!(ai.contains("%%Title: (EPS Test)"));
        assert!(ai.contains("moveto"));
    }

    fn text_project_with_system_font() -> Project {
        use crate::object::{TextAlignment, TextAlignmentV, TextFontSource, TextLayoutMode};
        let mut project = Project::new("EPS Text Test");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "label",
            layer_id,
            Bounds::new(Point2D::new(5.0, 5.0), Point2D::new(50.0, 15.0)),
            ObjectData::Text {
                content: "Hello EPS".to_string(),
                font_family: "Helvetica".to_string(),
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
                resolved_font_key: Some("Helvetica".to_string()),
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
    fn export_eps_text_exports_path_only() {
        let project = text_project_with_system_font();
        let eps = export_eps(&project, false, &[]).unwrap();
        assert!(
            !eps.contains("findfont"),
            "EPS should not emit PostScript text that duplicates path geometry"
        );
        assert!(
            !eps.contains(") show"),
            "EPS should not emit editable text show commands"
        );
        assert!(
            !eps.contains("Hello EPS"),
            "EPS should not emit text content separately"
        );
        assert!(
            eps.contains("moveto"),
            "EPS should contain text path geometry"
        );
    }

    #[test]
    fn export_ai_text_exports_path_only() {
        let project = text_project_with_system_font();
        let ai = export_ai(&project, false, &[]).unwrap();
        assert!(ai.contains("%%Creator: Adobe Illustrator"));
        assert!(
            !ai.contains("findfont"),
            "AI should not emit PostScript text that duplicates path geometry"
        );
        assert!(
            !ai.contains(") show"),
            "AI should not emit editable text show commands"
        );
        assert!(
            ai.contains("moveto"),
            "AI should contain text path geometry"
        );
    }

    #[test]
    fn export_eps_path_text_no_text_ops() {
        use crate::object::{TextAlignment, TextAlignmentV, TextFontSource, TextLayoutMode};
        let mut project = Project::new("Path Text EPS");
        let layer_id = project.ensure_default_layer();
        // Modern path-text
        project.add_object(ProjectObject::new(
            "path_text",
            layer_id,
            Bounds::new(Point2D::new(5.0, 5.0), Point2D::new(50.0, 15.0)),
            ObjectData::Text {
                content: "Curved EPS".to_string(),
                font_family: "Helvetica".to_string(),
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
                resolved_font_key: Some("Helvetica".to_string()),
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
                content: "Legacy EPS".to_string(),
                font_family: "Helvetica".to_string(),
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
                resolved_font_key: Some("Helvetica".to_string()),
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
        let eps = export_eps(&project, false, &[]).unwrap();
        assert!(
            !eps.contains("findfont"),
            "EPS should NOT contain findfont for path-text objects"
        );
        assert!(
            !eps.contains(") show"),
            "EPS should NOT contain text show for path-text objects"
        );
        assert!(
            eps.contains("moveto"),
            "EPS should contain moveto from path fallback"
        );
    }

    #[test]
    fn export_eps_text_missing_font_no_text_ops() {
        use crate::object::{TextAlignment, TextAlignmentV, TextLayoutMode};
        let mut project = Project::new("Missing Font EPS");
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
        let eps = export_eps(&project, false, &[]).unwrap();
        assert!(
            !eps.contains("findfont"),
            "EPS should NOT contain findfont for missing font"
        );
        assert!(
            !eps.contains(") show"),
            "EPS should NOT contain text show for missing font"
        );
        assert!(
            eps.contains("moveto"),
            "EPS should contain moveto from path fallback"
        );
    }

    #[test]
    fn export_eps_text_with_special_chars_exports_path_only() {
        use crate::object::{TextAlignment, TextAlignmentV, TextFontSource, TextLayoutMode};
        let mut project = Project::new("Escape Test");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "special",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(50.0, 10.0)),
            ObjectData::Text {
                content: "A (test) value".to_string(),
                font_family: "Courier".to_string(),
                font_size_mm: 5.0,
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
                resolved_font_key: Some("Courier".to_string()),
                resolved_path_data: None,
                missing_font: false,
                missing_glyphs: Vec::new(),
                guide_path_id: None,
                variable_text: None,
                max_width: None,
                squeeze: false,
                ignore_empty_vars: false,
            },
        ));
        let eps = export_eps(&project, false, &[]).unwrap();
        assert!(
            !eps.contains("A \\(test\\) value"),
            "EPS should not emit PostScript text strings separately"
        );
        assert!(eps.contains("moveto"), "EPS should export text as path");
    }

    #[test]
    fn export_eps_uppercase_text_exports_path_only() {
        use crate::object::{TextAlignment, TextAlignmentV, TextFontSource, TextLayoutMode};
        let mut project = Project::new("Styled EPS");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "styled",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(50.0, 10.0)),
            ObjectData::Text {
                content: "hello eps".to_string(),
                font_family: "Helvetica".to_string(),
                font_size_mm: 8.0,
                alignment: TextAlignment::Left,
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
                resolved_font_key: Some("Helvetica".to_string()),
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
        let eps = export_eps(&project, false, &[]).unwrap();
        assert!(
            !eps.contains("HELLO EPS"),
            "EPS should not emit editable text content separately"
        );
        assert!(eps.contains("moveto"), "EPS should export text as path");
    }
}

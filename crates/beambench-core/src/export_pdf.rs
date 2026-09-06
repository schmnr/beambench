//! Minimal PDF export for projects.

use crate::object::ObjectId;
use crate::project::Project;
use crate::vector::convert::object_to_world_vecpath;
use beambench_common::path::PathCommand;

fn pdf_stroke_color(color_tag: Option<&str>) -> (f64, f64, f64) {
    let Some(color_tag) = color_tag else {
        return (0.0, 0.0, 0.0);
    };

    let hex = match color_tag.strip_prefix('#') {
        Some(hex)
            if matches!(hex.len(), 6 | 8) && hex.bytes().all(|byte| byte.is_ascii_hexdigit()) =>
        {
            hex
        }
        _ => return (0.0, 0.0, 0.0),
    };

    let channel =
        |start| u8::from_str_radix(&hex[start..start + 2], 16).unwrap_or(0) as f64 / 255.0;
    (channel(0), channel(2), channel(4))
}

/// Export project as minimal PDF.
pub fn export_pdf(project: &Project, selection_only: bool, selected_ids: &[ObjectId]) -> Vec<u8> {
    let mut pdf = String::new();

    // PDF header
    pdf.push_str("%PDF-1.4\n");

    let mut object_offsets = Vec::new();

    // Catalog object (1 0 obj)
    object_offsets.push(pdf.len());
    pdf.push_str("1 0 obj\n<<\n/Type /Catalog\n/Pages 2 0 R\n>>\nendobj\n");

    // Pages object (2 0 obj)
    object_offsets.push(pdf.len());
    pdf.push_str("2 0 obj\n<<\n/Type /Pages\n/Kids [3 0 R]\n/Count 1\n>>\nendobj\n");

    // Page object (3 0 obj)
    object_offsets.push(pdf.len());
    let width = project.workspace.bed_width_mm * 2.83465; // mm to points (1mm ≈ 2.83465 pt)
    let height = project.workspace.bed_height_mm * 2.83465;
    pdf.push_str(&format!(
        "3 0 obj\n<<\n/Type /Page\n/Parent 2 0 R\n/MediaBox [0 0 {} {}]\n/Contents 4 0 R\n>>\nendobj\n",
        width, height
    ));

    // Content stream (4 0 obj)
    let mut stream = String::new();
    stream.push_str("0.1 w\n"); // Line width

    let expanded_clones: Vec<_> = project
        .objects
        .iter()
        .filter_map(|obj| project.resolve_clone(obj))
        .collect();
    for obj in project
        .objects
        .iter()
        .filter(|obj| !matches!(obj.data, crate::ObjectData::VirtualClone { .. }))
        .chain(expanded_clones.iter())
    {
        if selection_only && !selected_ids.contains(&obj.id) {
            continue;
        }

        if !obj.visible {
            continue;
        }

        if let Some(mut path) = object_to_world_vecpath(obj) {
            crate::vector::flatten::convert_quadratics_to_cubics(&mut path);
            let color_tag = project
                .find_layer(obj.layer_id)
                .map(|layer| layer.color_tag.0.as_str());
            let (red, green, blue) = pdf_stroke_color(color_tag);
            stream.push_str(&format!("{red:.6} {green:.6} {blue:.6} RG\n"));

            for subpath in &path.subpaths {
                for cmd in &subpath.commands {
                    match cmd {
                        PathCommand::MoveTo { x, y } => {
                            let px = *x * 2.83465;
                            let py = *y * 2.83465;
                            stream.push_str(&format!("{} {} m\n", px, py));
                        }
                        PathCommand::LineTo { x, y } => {
                            let px = *x * 2.83465;
                            let py = *y * 2.83465;
                            stream.push_str(&format!("{} {} l\n", px, py));
                        }
                        PathCommand::QuadTo { .. } => {
                            unreachable!("quadratics were converted above")
                        }
                        PathCommand::CubicTo {
                            c1x,
                            c1y,
                            c2x,
                            c2y,
                            x,
                            y,
                        } => {
                            let pc1x = *c1x * 2.83465;
                            let pc1y = *c1y * 2.83465;
                            let pc2x = *c2x * 2.83465;
                            let pc2y = *c2y * 2.83465;
                            let px = *x * 2.83465;
                            let py = *y * 2.83465;
                            stream.push_str(&format!(
                                "{} {} {} {} {} {} c\n",
                                pc1x, pc1y, pc2x, pc2y, px, py
                            ));
                        }
                        PathCommand::Close => {
                            stream.push_str("h\n");
                        }
                    }
                }
                stream.push_str("S\n"); // Stroke path
            }
        }
    }

    let stream_len = stream.len();
    object_offsets.push(pdf.len());
    pdf.push_str(&format!(
        "4 0 obj\n<<\n/Length {}\n>>\nstream\n{}endstream\nendobj\n",
        stream_len, stream
    ));

    // Cross-reference offsets are byte positions in the final ASCII document.
    let xref_offset = pdf.len();
    pdf.push_str("xref\n0 5\n0000000000 65535 f \n");
    for offset in object_offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str("trailer\n<<\n/Size 5\n/Root 1 0 R\n>>\nstartxref\n");
    pdf.push_str(&format!("{xref_offset}\n%%EOF\n"));

    pdf.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::{Layer, OperationType};
    use crate::object::{ObjectData, ProjectObject};
    use beambench_common::{Bounds, ColorTag, Point2D};

    fn rectangle(name: &str, layer_id: crate::layer::LayerId, x: f64) -> ProjectObject {
        ProjectObject::new(
            name,
            layer_id,
            Bounds::new(Point2D::new(x, 10.0), Point2D::new(x + 10.0, 20.0)),
            ObjectData::Shape {
                kind: crate::object::ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        )
    }

    fn test_project() -> Project {
        let mut project = Project::new("PDF Export Test");
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
        project.add_object(obj);
        project
    }

    #[test]
    fn export_pdf_has_header() {
        let project = test_project();
        let pdf = export_pdf(&project, false, &[]);
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF"));
    }

    #[test]
    fn export_pdf_has_catalog() {
        let project = test_project();
        let pdf = export_pdf(&project, false, &[]);
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Catalog"));
    }

    #[test]
    fn export_pdf_has_eof() {
        let project = test_project();
        let pdf = export_pdf(&project, false, &[]);
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("%%EOF"));
    }

    #[test]
    fn export_pdf_uses_each_objects_layer_stroke_color() {
        let mut project = Project::new("Colored PDF Export Test");
        let mut red_layer = Layer::new_single_entry("Red", OperationType::Line);
        red_layer.color_tag = ColorTag("#FF0000".to_string());
        let red_layer_id = project.add_layer(red_layer).id;

        let mut blue_layer = Layer::new_single_entry("Blue", OperationType::Line);
        blue_layer.color_tag = ColorTag("#0080FFFF".to_string());
        let blue_layer_id = project.add_layer(blue_layer).id;

        project.add_object(rectangle("red rectangle", red_layer_id, 10.0));
        project.add_object(rectangle("blue rectangle", blue_layer_id, 30.0));

        let pdf = export_pdf(&project, false, &[]);
        let text = String::from_utf8_lossy(&pdf);

        assert!(text.contains("1.000000 0.000000 0.000000 RG"));
        assert!(text.contains("0.000000 0.501961 1.000000 RG"));
    }

    #[test]
    fn selection_only_emits_only_the_selected_objects_layer_color() {
        let mut project = Project::new("Selected Color PDF Export Test");
        let mut red_layer = Layer::new_single_entry("Red", OperationType::Line);
        red_layer.color_tag = ColorTag("#FF0000".to_string());
        let red_layer_id = project.add_layer(red_layer).id;

        let mut green_layer = Layer::new_single_entry("Green", OperationType::Line);
        green_layer.color_tag = ColorTag("#00FF00".to_string());
        let green_layer_id = project.add_layer(green_layer).id;

        let red_object = rectangle("red rectangle", red_layer_id, 10.0);
        let green_object = rectangle("green rectangle", green_layer_id, 30.0);
        let green_object_id = green_object.id;
        project.add_object(red_object);
        project.add_object(green_object);

        let pdf = export_pdf(&project, true, &[green_object_id]);
        let text = String::from_utf8_lossy(&pdf);

        assert!(!text.contains("1.000000 0.000000 0.000000 RG"));
        assert!(text.contains("0.000000 1.000000 0.000000 RG"));
    }

    #[test]
    fn invalid_or_missing_layer_colors_fall_back_to_black() {
        assert_eq!(pdf_stroke_color(None), (0.0, 0.0, 0.0));
        assert_eq!(pdf_stroke_color(Some("not-a-color")), (0.0, 0.0, 0.0));
        assert_eq!(pdf_stroke_color(Some("#FF0000ZZ")), (0.0, 0.0, 0.0));
    }
    #[test]
    fn xref_points_to_objects_and_quadratics_remain_curves() {
        let mut project = test_project();
        project.objects[0].data = ObjectData::VectorPath {
            path_data: "M10 10 Q35 60 60 10".into(),
            closed: false,
            ruler_guide_axis: None,
        };
        let text = String::from_utf8(export_pdf(&project, false, &[])).unwrap();
        assert!(text.contains(" c\n"));
        let xref: usize = text
            .split("startxref\n")
            .nth(1)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(text[xref..].starts_with("xref\n"));
        for (index, line) in text[xref..].lines().skip(3).take(4).enumerate() {
            let offset: usize = line.split_whitespace().next().unwrap().parse().unwrap();
            assert!(text[offset..].starts_with(&format!("{} 0 obj\n", index + 1)));
        }
        assert!(crate::export_eps(&project, false, &[]).contains("curveto"));
    }
}

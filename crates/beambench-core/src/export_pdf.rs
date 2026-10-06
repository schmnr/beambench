//! Minimal PDF export for projects.

use crate::export_bitmap::processed_bitmap_image_for_object;
use crate::export_common::{
    POINTS_PER_MM, canvas_to_y_up, exportable_objects, matrix_operands,
    raster_unit_square_to_canvas,
};
use crate::object::{ObjectData, ObjectId};
use crate::project::Project;
use crate::vector::convert::object_to_world_vecpath;
use beambench_common::Point2D;
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

/// Export project as a minimal PDF. Raster images are embedded as grayscale
/// image objects, exactly as they will engrave.
pub fn export_pdf(
    project: &Project,
    selection_only: bool,
    selected_ids: &[ObjectId],
) -> Result<Vec<u8>, String> {
    let to_page = canvas_to_y_up(project.workspace.bed_height_mm, POINTS_PER_MM);
    let mut stream = String::new();
    stream.push_str("0.1 w\n"); // Line width
    let mut images: Vec<Vec<u8>> = Vec::new();

    for obj in exportable_objects(project, selection_only, selected_ids) {
        if matches!(obj.data, ObjectData::RasterImage { .. }) {
            let image = processed_bitmap_image_for_object(project, &obj)?;
            let placement = to_page.compose(&raster_unit_square_to_canvas(&obj));
            stream.push_str(&format!(
                "q\n{} cm\n/Im{} Do\nQ\n",
                matrix_operands(&placement),
                images.len()
            ));
            images.push(pdf_gray_image_object(&image)?);
            continue;
        }
        let Some(mut path) = object_to_world_vecpath(&obj) else {
            continue;
        };
        crate::vector::flatten::convert_quadratics_to_cubics(&mut path);
        let color_tag = project
            .find_layer(obj.layer_id)
            .map(|layer| layer.color_tag.0.as_str());
        let (red, green, blue) = pdf_stroke_color(color_tag);
        stream.push_str(&format!("{red:.6} {green:.6} {blue:.6} RG\n"));
        let point = |x: f64, y: f64| {
            let p = to_page.apply(&Point2D::new(x, y));
            format!("{} {}", p.x, p.y)
        };
        for subpath in &path.subpaths {
            for cmd in &subpath.commands {
                match cmd {
                    PathCommand::MoveTo { x, y } => {
                        stream.push_str(&format!("{} m\n", point(*x, *y)));
                    }
                    PathCommand::LineTo { x, y } => {
                        stream.push_str(&format!("{} l\n", point(*x, *y)));
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
                        stream.push_str(&format!(
                            "{} {} {} c\n",
                            point(*c1x, *c1y),
                            point(*c2x, *c2y),
                            point(*x, *y)
                        ));
                    }
                    PathCommand::Close => stream.push_str("h\n"),
                }
            }
            stream.push_str("S\n"); // Stroke path
        }
    }

    let width = project.workspace.bed_width_mm * POINTS_PER_MM;
    let height = project.workspace.bed_height_mm * POINTS_PER_MM;
    let first_image = 5;
    let xobjects: String = (0..images.len())
        .map(|index| format!("/Im{index} {} 0 R ", first_image + index))
        .collect();

    let mut pdf: Vec<u8> = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    let mut push_object = |pdf: &mut Vec<u8>, body: &[u8]| {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    };
    push_object(&mut pdf, b"<<\n/Type /Catalog\n/Pages 2 0 R\n>>");
    push_object(&mut pdf, b"<<\n/Type /Pages\n/Kids [3 0 R]\n/Count 1\n>>");
    push_object(
        &mut pdf,
        format!(
            "<<\n/Type /Page\n/Parent 2 0 R\n/MediaBox [0 0 {width} {height}]\n/Resources << /XObject << {xobjects}>> >>\n/Contents 4 0 R\n>>"
        )
        .as_bytes(),
    );
    push_object(
        &mut pdf,
        format!(
            "<<\n/Length {}\n>>\nstream\n{stream}endstream",
            stream.len()
        )
        .as_bytes(),
    );
    for image in &images {
        push_object(&mut pdf, image);
    }

    // Cross-reference offsets are byte positions in the final document.
    let xref_offset = pdf.len();
    let count = offsets.len() + 1;
    pdf.extend_from_slice(format!("xref\n0 {count}\n0000000000 65535 f \n").as_bytes());
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<<\n/Size {count}\n/Root 1 0 R\n>>\nstartxref\n{xref_offset}\n%%EOF\n")
            .as_bytes(),
    );
    Ok(pdf)
}

/// A Flate-compressed 8-bit grayscale image object body.
fn pdf_gray_image_object(image: &image::GrayImage) -> Result<Vec<u8>, String> {
    use std::io::Write;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(image.as_raw())
        .and_then(|_| encoder.flush())
        .map_err(|e| format!("Failed to compress image: {e}"))?;
    let data = encoder
        .finish()
        .map_err(|e| format!("Failed to compress image: {e}"))?;
    let mut body = format!(
        "<<\n/Type /XObject\n/Subtype /Image\n/Width {}\n/Height {}\n/ColorSpace /DeviceGray\n/BitsPerComponent 8\n/Filter /FlateDecode\n/Length {}\n>>\nstream\n",
        image.width(),
        image.height(),
        data.len()
    )
    .into_bytes();
    body.extend_from_slice(&data);
    body.extend_from_slice(b"\nendstream");
    Ok(body)
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
        let pdf = export_pdf(&project, false, &[]).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF"));
    }

    #[test]
    fn export_pdf_has_catalog() {
        let project = test_project();
        let pdf = export_pdf(&project, false, &[]).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Catalog"));
    }

    #[test]
    fn export_pdf_has_eof() {
        let project = test_project();
        let pdf = export_pdf(&project, false, &[]).unwrap();
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

        let pdf = export_pdf(&project, false, &[]).unwrap();
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

        let pdf = export_pdf(&project, true, &[green_object_id]).unwrap();
        let text = String::from_utf8_lossy(&pdf);

        assert!(!text.contains("1.000000 0.000000 0.000000 RG"));
        assert!(text.contains("0.000000 1.000000 0.000000 RG"));
    }

    fn raster_project() -> Project {
        use image::ImageEncoder;
        let mut project = test_project();
        project.workspace.bed_height_mm = 200.0;
        let layer_id = project.layers[0].id;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[0, 85, 170, 255], 2, 2, image::ExtendedColorType::L8)
            .unwrap();
        let asset = crate::asset::Asset::new(
            "photo.png",
            crate::asset::AssetMediaType::Png,
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
        raster.transform = beambench_common::Transform2D::translate(5.0, 6.0);
        project.add_object(raster);
        project
    }

    #[test]
    fn images_are_embedded_where_they_sit_on_the_canvas() {
        let project = raster_project();
        let pdf = export_pdf(&project, false, &[]).unwrap();

        let artwork = crate::parse_pdf_artwork(&pdf).unwrap();
        assert_eq!(artwork.images.len(), 1);
        let to_canvas = artwork.page_to_canvas.compose(&artwork.images[0].transform);
        let center = to_canvas.apply(&Point2D::new(0.5, 0.5));
        assert!((center.x - 125.0).abs() < 1e-3, "{center:?}");
        assert!((center.y - 66.0).abs() < 1e-3, "{center:?}");
        assert!(!artwork.paths.is_empty());

        let eps = crate::export_eps(&project, false, &[]).unwrap();
        assert!(eps.contains("} image\n"));
    }

    #[test]
    fn vectors_keep_their_canvas_position_after_reimport() {
        let mut project = test_project();
        project.workspace.bed_height_mm = 200.0;
        let pdf = export_pdf(&project, false, &[]).unwrap();
        let artwork = crate::parse_pdf_artwork(&pdf).unwrap();
        let path = artwork.paths[0].path.clone();
        let mut ys: Vec<f64> = path
            .subpaths
            .iter()
            .flat_map(|subpath| subpath.commands.iter())
            .filter_map(|command| match command {
                PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => {
                    Some(artwork.page_to_canvas.apply(&Point2D::new(*x, *y)).y)
                }
                _ => None,
            })
            .collect();
        ys.sort_by(f64::total_cmp);
        assert!((ys[0] - 10.0).abs() < 1e-3 && (ys[ys.len() - 1] - 60.0).abs() < 1e-3);
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
        let text = String::from_utf8(export_pdf(&project, false, &[]).unwrap()).unwrap();
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
        assert!(
            crate::export_eps(&project, false, &[])
                .unwrap()
                .contains("curveto")
        );
    }
}

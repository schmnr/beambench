//! Print document rendering for the File menu print commands.

use base64::{Engine as _, engine::general_purpose};
use beambench_common::Transform2D;
use beambench_common::path::VecPath;
use image::ImageEncoder;

use crate::export_bitmap::processed_bitmap_png_for_object;
use crate::object::{ObjectData, ObjectId, ProjectObject};
use crate::project::Project;
use crate::vector::{bake_transform, convert::object_to_world_vecpath};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintMode {
    Black,
    Color,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintDocument {
    pub title: String,
    pub svg: String,
}

/// Render the current workspace as a print-ready SVG.
pub fn render_print_document(project: &Project, mode: PrintMode) -> Result<PrintDocument, String> {
    render_print_document_with_selection(project, mode, false, &[])
}

/// Render the current workspace or selected objects as a visual SVG.
///
/// Unlike the laser-oriented SVG export, this embeds raster images as data URLs so
/// the output is suitable for agent/human visual review of the canvas.
pub fn render_print_document_with_selection(
    project: &Project,
    mode: PrintMode,
    selection_only: bool,
    selected_ids: &[ObjectId],
) -> Result<PrintDocument, String> {
    let width = project.workspace.bed_width_mm;
    let height = project.workspace.bed_height_mm;
    let mut svg = String::new();
    let title = project.metadata.project_name.clone();

    svg.push_str(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}mm" height="{}mm" viewBox="0 0 {} {}">"#,
        fmt_num(width),
        fmt_num(height),
        fmt_num(width),
        fmt_num(height),
    ));
    svg.push('\n');
    svg.push_str(&format!("  <title>{}</title>\n", escape_text(&title)));

    for obj in printable_objects(project) {
        if selection_only && !selected_ids.contains(&obj.id) {
            continue;
        }
        if !obj.visible {
            continue;
        }
        let Some(layer) = project
            .find_layer(obj.layer_id)
            .filter(|layer| layer.visible)
        else {
            continue;
        };
        let color = print_color(mode, &layer.color_tag.0);

        match &obj.data {
            ObjectData::RasterImage { .. } => {
                if let Some(image) = raster_image_svg(project, &obj)? {
                    svg.push_str(&image);
                }
            }
            ObjectData::Barcode {
                barcode_type,
                data,
                width,
                height,
                options,
            } => {
                let path = crate::barcode_gen::generate_barcode_with_options(
                    *barcode_type,
                    data,
                    *width,
                    *height,
                    options,
                )
                .map(|path| map_local_path_to_object_world(path, &obj))?;
                svg.push_str(&format!(
                    r#"  <path d="{}" fill="{}" stroke="none"/>"#,
                    escape_attr(&path.to_svg_d()),
                    color,
                ));
                svg.push('\n');
            }
            _ => {
                if let Some(path) = object_to_world_vecpath(&obj) {
                    svg.push_str(&format!(
                        r#"  <path d="{}" fill="none" stroke="{}" stroke-width="0.1"/>"#,
                        escape_attr(&path.to_svg_d()),
                        color,
                    ));
                    svg.push('\n');
                }
            }
        }
    }

    svg.push_str("</svg>\n");
    Ok(PrintDocument { title, svg })
}

/// Render the current workspace or selected objects as PNG bytes.
pub fn render_print_png(
    project: &Project,
    mode: PrintMode,
    selection_only: bool,
    selected_ids: &[ObjectId],
    pixels_per_mm: f64,
) -> Result<Vec<u8>, String> {
    let pixels_per_mm = if pixels_per_mm.is_finite() && pixels_per_mm > 0.0 {
        pixels_per_mm
    } else {
        4.0
    };
    let width_px = ((project.workspace.bed_width_mm * pixels_per_mm).round() as u32).max(1);
    let height_px = ((project.workspace.bed_height_mm * pixels_per_mm).round() as u32).max(1);
    let document =
        render_print_document_with_selection(project, mode, selection_only, selected_ids)?;
    render_svg_document_to_png(&document.svg, width_px, height_px)
}

fn render_svg_document_to_png(svg: &str, width_px: u32, height_px: u32) -> Result<Vec<u8>, String> {
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg.as_bytes(), &options)
        .map_err(|e| format!("Failed to parse visual SVG for PNG render: {e}"))?;
    let tree_size = tree.size();
    let mut pixmap = tiny_skia::Pixmap::new(width_px, height_px)
        .ok_or_else(|| "Failed to allocate PNG render surface".to_string())?;
    pixmap.fill(tiny_skia::Color::WHITE);
    let scale_x = width_px as f32 / tree_size.width();
    let scale_y = height_px as f32 / tree_size.height();
    let mut pixmap_mut = pixmap.as_mut();
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale_x, scale_y),
        &mut pixmap_mut,
    );

    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            pixmap.data(),
            width_px,
            height_px,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("Failed to encode PNG render: {e}"))?;
    Ok(bytes)
}

fn printable_objects(project: &Project) -> Vec<ProjectObject> {
    project
        .objects
        .iter()
        .filter_map(|obj| {
            if matches!(obj.data, ObjectData::VirtualClone { .. }) {
                project.resolve_clone(obj)
            } else {
                Some(obj.clone())
            }
        })
        .collect()
}

fn print_color(mode: PrintMode, layer_color: &str) -> String {
    match mode {
        PrintMode::Black => "#000000".to_string(),
        PrintMode::Color => {
            let trimmed = layer_color.trim();
            if is_css_hex_color(trimmed) {
                trimmed.to_string()
            } else {
                "#000000".to_string()
            }
        }
    }
}

fn is_css_hex_color(value: &str) -> bool {
    let hex = value.strip_prefix('#').unwrap_or_default();
    matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn raster_image_svg(project: &Project, obj: &ProjectObject) -> Result<Option<String>, String> {
    if !matches!(obj.data, ObjectData::RasterImage { .. }) {
        return Ok(None);
    }
    let bytes = processed_bitmap_png_for_object(project, obj)?;
    let data = general_purpose::STANDARD.encode(bytes);
    let transform = svg_transform_attr(&obj.transform);
    Ok(Some(
        format!(
            r#"  <image x="{}" y="{}" width="{}" height="{}" href="data:image/png;base64,{}" preserveAspectRatio="none"{}/>"#,
            fmt_num(obj.bounds.min.x),
            fmt_num(obj.bounds.min.y),
            fmt_num(obj.bounds.max.x - obj.bounds.min.x),
            fmt_num(obj.bounds.max.y - obj.bounds.min.y),
            data,
            transform,
        ) + "\n",
    ))
}

fn map_local_path_to_object_world(path: VecPath, obj: &ProjectObject) -> VecPath {
    let Some(intrinsic) = path.visual_bounds().or_else(|| path.bounds()) else {
        return path;
    };
    let old_w = intrinsic.max.x - intrinsic.min.x;
    let old_h = intrinsic.max.y - intrinsic.min.y;
    let new_w = obj.bounds.max.x - obj.bounds.min.x;
    let new_h = obj.bounds.max.y - obj.bounds.min.y;

    let sx = if old_w > 0.0 { new_w / old_w } else { 1.0 };
    let sy = if old_h > 0.0 { new_h / old_h } else { 1.0 };
    let tx = obj.bounds.min.x - intrinsic.min.x * sx;
    let ty = obj.bounds.min.y - intrinsic.min.y * sy;

    let mapped = bake_transform(
        &path,
        &Transform2D {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            tx,
            ty,
        },
    );

    if obj.transform.is_identity() {
        mapped
    } else {
        bake_transform(&mapped, &obj.transform)
    }
}

fn svg_transform_attr(transform: &Transform2D) -> String {
    if transform.is_identity() {
        String::new()
    } else {
        format!(
            r#" transform="matrix({} {} {} {} {} {})""#,
            fmt_num(transform.a),
            fmt_num(transform.b),
            fmt_num(transform.c),
            fmt_num(transform.d),
            fmt_num(transform.tx),
            fmt_num(transform.ty),
        )
    }
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

fn escape_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::{Asset, AssetMediaType};
    use crate::layer::{Layer, OperationType};
    use crate::object::{ObjectData, ProjectObject, ShapeKind};
    use beambench_common::{Bounds, ColorTag, Point2D, RasterAdjustments};
    use image::ImageEncoder;

    fn test_project() -> Project {
        let mut project = Project::new("Print Test");
        project.workspace.bed_width_mm = 250.0;
        project.workspace.bed_height_mm = 125.0;
        project
    }

    fn rect(name: &str, layer_id: crate::LayerId, x: f64) -> ProjectObject {
        ProjectObject::new(
            name,
            layer_id,
            Bounds::new(Point2D::new(x, 10.0), Point2D::new(x + 20.0, 30.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 20.0,
                height: 20.0,
                corner_radius: 0.0,
            },
        )
    }

    #[test]
    fn export_print_black_mode_uses_black_strokes_only() {
        let mut project = test_project();
        let layer_id = project.ensure_default_layer();
        project.find_layer_mut(layer_id).unwrap().color_tag = ColorTag("#FF0000".to_string());
        project.add_object(rect("rect", layer_id, 10.0));

        let doc = render_print_document(&project, PrintMode::Black).unwrap();

        assert!(doc.svg.contains(r##"stroke="#000000""##));
        assert!(!doc.svg.contains("#FF0000"));
    }

    #[test]
    fn export_print_color_mode_uses_layer_colors() {
        let mut project = test_project();
        let layer_id = project.ensure_default_layer();
        project.find_layer_mut(layer_id).unwrap().color_tag = ColorTag("#00AAFF".to_string());
        project.add_object(rect("rect", layer_id, 10.0));

        let doc = render_print_document(&project, PrintMode::Color).unwrap();

        assert!(doc.svg.contains(r##"stroke="#00AAFF""##));
    }

    #[test]
    fn export_print_omits_hidden_layers_and_objects() {
        let mut project = test_project();
        let visible_layer_id = project.ensure_default_layer();
        let mut hidden_layer = Layer::new("Hidden", OperationType::Line);
        hidden_layer.visible = false;
        let hidden_layer_id = hidden_layer.id;
        project.add_layer(hidden_layer);

        project.add_object(rect("visible", visible_layer_id, 10.0));
        project.add_object(rect("hidden-layer", hidden_layer_id, 40.0));
        let mut hidden_object = rect("hidden-object", visible_layer_id, 70.0);
        hidden_object.visible = false;
        project.add_object(hidden_object);

        let doc = render_print_document(&project, PrintMode::Color).unwrap();

        assert_eq!(doc.svg.matches("<path ").count(), 1);
    }

    #[test]
    fn export_print_svg_uses_workspace_dimensions_in_mm() {
        let project = test_project();

        let doc = render_print_document(&project, PrintMode::Black).unwrap();

        assert!(doc.svg.contains(r#"width="250mm""#));
        assert!(doc.svg.contains(r#"height="125mm""#));
        assert!(doc.svg.contains(r#"viewBox="0 0 250 125""#));
    }

    #[test]
    fn export_print_embeds_raster_images_as_png_data_urls() {
        let mut project = test_project();
        let img = image::GrayImage::from_pixel(2, 2, image::Luma([128u8]));
        let mut png_bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png_bytes)
            .write_image(img.as_raw(), 2, 2, image::ExtendedColorType::L8)
            .unwrap();
        let asset = Asset::new(
            "image.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(2),
            Some(2),
        );
        let asset_key = asset.id.to_string();
        project.add_asset(asset, png_bytes);

        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "raster",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(30.0, 30.0)),
            ObjectData::RasterImage {
                asset_key,
                original_width_px: 2,
                original_height_px: 2,
                adjustments: Some(RasterAdjustments::default()),
                masks: Vec::new(),
            },
        ));

        let doc = render_print_document(&project, PrintMode::Black).unwrap();

        assert!(doc.svg.contains(r#"<image "#));
        assert!(doc.svg.contains("href=\"data:image/png;base64,"));
    }

    #[test]
    fn export_print_png_includes_raster_images() {
        let mut project = test_project();
        project.workspace.bed_width_mm = 20.0;
        project.workspace.bed_height_mm = 20.0;
        let img = image::GrayImage::from_pixel(4, 4, image::Luma([0u8]));
        let mut png_bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png_bytes)
            .write_image(img.as_raw(), 4, 4, image::ExtendedColorType::L8)
            .unwrap();
        let asset = Asset::new(
            "image.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(4),
            Some(4),
        );
        let asset_key = asset.id.to_string();
        project.add_asset(asset, png_bytes);

        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "raster",
            layer_id,
            Bounds::new(Point2D::new(2.0, 2.0), Point2D::new(18.0, 18.0)),
            ObjectData::RasterImage {
                asset_key,
                original_width_px: 4,
                original_height_px: 4,
                adjustments: Some(RasterAdjustments::default()),
                masks: Vec::new(),
            },
        ));

        let png = render_print_png(&project, PrintMode::Black, false, &[], 4.0).unwrap();
        let decoded = image::load_from_memory(&png).unwrap().to_rgba8();

        assert_eq!(decoded.width(), 80);
        assert_eq!(decoded.height(), 80);
        assert!(
            decoded
                .pixels()
                .any(|pixel| pixel.0[0] < 250 || pixel.0[1] < 250 || pixel.0[2] < 250),
            "PNG render should contain non-white pixels from the raster image"
        );
    }
}

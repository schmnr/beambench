//! Print document rendering for the File menu print commands.

use std::collections::HashSet;

use base64::{Engine as _, engine::general_purpose};
use beambench_common::path::VecPath;
use beambench_common::{Bounds, Point2D, Transform2D};
use image::ImageEncoder;

use crate::export_bitmap::processed_bitmap_png_for_object;
use crate::layer::{Layer, OperationType};
use crate::object::{ObjectData, ObjectId, ProjectObject};
use crate::project::Project;
use crate::vector::convert::object_to_world_vecpath;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintMode {
    Black,
    Color,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PrintAppearance {
    #[default]
    Operation,
    Outline,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrintDocument {
    pub title: String,
    pub svg: String,
    pub width_mm: f64,
    pub height_mm: f64,
}

/// Render the current workspace as a print-ready SVG.
pub fn render_print_document(project: &Project, mode: PrintMode) -> Result<PrintDocument, String> {
    render_print_document_with_options(project, mode, PrintAppearance::Operation, false, &[], false)
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
    render_print_document_with_options(
        project,
        mode,
        PrintAppearance::Operation,
        selection_only,
        selected_ids,
        selection_only,
    )
}

pub fn render_print_document_with_options(
    project: &Project,
    mode: PrintMode,
    appearance: PrintAppearance,
    selection_only: bool,
    selected_ids: &[ObjectId],
    crop_to_content: bool,
) -> Result<PrintDocument, String> {
    let objects: Vec<_> = printable_objects(project)
        .into_iter()
        .filter(|obj| !selection_only || selected_ids.contains(&obj.id))
        .filter(|obj| obj.visible)
        .filter(|obj| {
            project
                .find_layer(obj.layer_id)
                .is_some_and(|layer| layer.visible)
        })
        .collect();
    let bounds = if crop_to_content {
        content_bounds(&objects).unwrap_or_else(|| workspace_bounds(project))
    } else {
        workspace_bounds(project)
    };
    let width = bounds.width().max(0.01);
    let height = bounds.height().max(0.01);
    let mut svg = String::new();
    let title = project.metadata.project_name.clone();

    svg.push_str(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}mm" height="{}mm" viewBox="{} {} {} {}">"#,
        fmt_num(width),
        fmt_num(height),
        fmt_num(bounds.min.x),
        fmt_num(bounds.min.y),
        fmt_num(width),
        fmt_num(height),
    ));
    svg.push('\n');
    svg.push_str(&format!("  <title>{}</title>\n", escape_text(&title)));

    let mut emitted_filled_layers = HashSet::new();
    for obj in &objects {
        let Some(layer) = project
            .find_layer(obj.layer_id)
            .filter(|layer| layer.visible)
        else {
            continue;
        };
        let color = print_color(mode, &layer.color_tag.0);

        if appearance == PrintAppearance::Operation
            && layer_uses_filled_appearance(layer)
            && !matches!(obj.data, ObjectData::RasterImage { .. })
        {
            if emitted_filled_layers.insert(layer.id) {
                let mut paths = Vec::new();
                for layer_object in objects
                    .iter()
                    .filter(|candidate| candidate.layer_id == layer.id)
                {
                    if let Some(path) = object_world_path(layer_object)? {
                        paths.push(path.to_svg_d());
                    }
                }
                if !paths.is_empty() {
                    svg.push_str(&format!(
                        r#"  <path d="{}" fill="{}" fill-rule="evenodd" fill-opacity="{}" stroke="none"/>"#,
                        escape_attr(&paths.join(" ")),
                        color,
                        fmt_num(layer.fill_opacity.clamp(0.0, 1.0)),
                    ));
                    svg.push('\n');
                }
            }
            continue;
        }

        match &obj.data {
            ObjectData::RasterImage { .. } => {
                if let Some(image) = raster_image_svg(project, &obj)? {
                    svg.push_str(&image);
                }
            }
            ObjectData::Barcode { .. } => {
                if let Some(path) = object_world_path(obj)? {
                    svg.push_str(&format!(
                        r#"  <path d="{}" fill="{}" stroke="none"/>"#,
                        escape_attr(&path.to_svg_d()),
                        color,
                    ));
                    svg.push('\n');
                }
            }
            _ => {
                if let Some(path) = object_world_path(obj)? {
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
    Ok(PrintDocument {
        title,
        svg,
        width_mm: width,
        height_mm: height,
    })
}

/// Render the current workspace or selected objects as PNG bytes.
pub fn render_print_png(
    project: &Project,
    mode: PrintMode,
    selection_only: bool,
    selected_ids: &[ObjectId],
    pixels_per_mm: f64,
) -> Result<Vec<u8>, String> {
    render_print_png_with_options(
        project,
        mode,
        PrintAppearance::Operation,
        selection_only,
        selected_ids,
        selection_only,
        pixels_per_mm,
    )
}

pub fn render_print_png_with_options(
    project: &Project,
    mode: PrintMode,
    appearance: PrintAppearance,
    selection_only: bool,
    selected_ids: &[ObjectId],
    crop_to_content: bool,
    pixels_per_mm: f64,
) -> Result<Vec<u8>, String> {
    let pixels_per_mm = if pixels_per_mm.is_finite() && pixels_per_mm > 0.0 {
        pixels_per_mm
    } else {
        4.0
    };
    let document = render_print_document_with_options(
        project,
        mode,
        appearance,
        selection_only,
        selected_ids,
        crop_to_content,
    )?;
    let width_px = ((document.width_mm * pixels_per_mm).round() as u32).max(1);
    let height_px = ((document.height_mm * pixels_per_mm).round() as u32).max(1);
    render_svg_document_to_png(&document.svg, width_px, height_px)
}

fn workspace_bounds(project: &Project) -> Bounds {
    Bounds::new(
        Point2D::zero(),
        Point2D::new(
            project.workspace.bed_width_mm,
            project.workspace.bed_height_mm,
        ),
    )
}

fn content_bounds(objects: &[ProjectObject]) -> Option<Bounds> {
    let mut combined: Option<Bounds> = None;
    for object in objects {
        let center = Point2D::new(
            (object.bounds.min.x + object.bounds.max.x) / 2.0,
            (object.bounds.min.y + object.bounds.max.y) / 2.0,
        );
        let corners = [
            object.bounds.min,
            Point2D::new(object.bounds.max.x, object.bounds.min.y),
            object.bounds.max,
            Point2D::new(object.bounds.min.x, object.bounds.max.y),
        ];
        let mut min = Point2D::new(f64::INFINITY, f64::INFINITY);
        let mut max = Point2D::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        for corner in corners {
            let point = object.transform.apply_around_center(&corner, &center);
            min.x = min.x.min(point.x);
            min.y = min.y.min(point.y);
            max.x = max.x.max(point.x);
            max.y = max.y.max(point.y);
        }
        let object_bounds = Bounds::new(min, max);
        combined = Some(combined.map_or(object_bounds, |bounds| bounds.union(&object_bounds)));
    }
    combined.map(|bounds| {
        const PADDING_MM: f64 = 1.0;
        Bounds::new(
            Point2D::new(bounds.min.x - PADDING_MM, bounds.min.y - PADDING_MM),
            Point2D::new(bounds.max.x + PADDING_MM, bounds.max.y + PADDING_MM),
        )
    })
}

fn layer_uses_filled_appearance(layer: &Layer) -> bool {
    !layer.is_tool_layer
        && layer.entries.iter().any(|entry| {
            matches!(
                entry.operation,
                OperationType::Image | OperationType::Fill | OperationType::OffsetFill
            )
        })
}

fn object_world_path(obj: &ProjectObject) -> Result<Option<VecPath>, String> {
    Ok(object_to_world_vecpath(obj))
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
    let transform = svg_transform_attr_around_bounds_center(&obj.transform, &obj.bounds);
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

fn svg_transform_attr_around_bounds_center(transform: &Transform2D, bounds: &Bounds) -> String {
    if transform.is_identity() {
        String::new()
    } else {
        let center_x = (bounds.min.x + bounds.max.x) / 2.0;
        let center_y = (bounds.min.y + bounds.max.y) / 2.0;
        let tx = transform.tx + center_x - transform.a * center_x - transform.c * center_y;
        let ty = transform.ty + center_y - transform.b * center_x - transform.d * center_y;
        format!(
            r#" transform="matrix({} {} {} {} {} {})""#,
            fmt_num(transform.a),
            fmt_num(transform.b),
            fmt_num(transform.c),
            fmt_num(transform.d),
            fmt_num(tx),
            fmt_num(ty),
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
    fn operation_appearance_combines_fill_layer_paths_with_even_odd_rule() {
        let mut project = test_project();
        let mut layer = Layer::new("Fill", OperationType::Fill);
        layer.color_tag = ColorTag("#FF0000".to_string());
        let layer_id = layer.id;
        project.add_layer(layer);
        project.add_object(rect("outer", layer_id, 10.0));
        project.add_object(rect("inner", layer_id, 15.0));

        let doc = render_print_document(&project, PrintMode::Color).unwrap();

        assert_eq!(doc.svg.matches("<path ").count(), 1);
        assert!(doc.svg.contains(r#"fill-rule="evenodd""#));
        assert!(doc.svg.contains(r##"fill="#FF0000""##));
        assert!(!doc.svg.contains("fill=\"none\""));
    }

    #[test]
    fn outline_appearance_keeps_fill_layer_as_wireframe() {
        let mut project = test_project();
        let layer = Layer::new("Fill", OperationType::Fill);
        let layer_id = layer.id;
        project.add_layer(layer);
        project.add_object(rect("rect", layer_id, 10.0));

        let doc = render_print_document_with_options(
            &project,
            PrintMode::Black,
            PrintAppearance::Outline,
            false,
            &[],
            false,
        )
        .unwrap();

        assert!(doc.svg.contains(r#"fill="none""#));
        assert!(doc.svg.contains(r##"stroke="#000000""##));
    }

    #[test]
    fn selection_render_crops_to_selected_content() {
        let mut project = test_project();
        let layer_id = project.ensure_default_layer();
        let selected_id = project.add_object(rect("selected", layer_id, 10.0)).id;
        project.add_object(rect("other", layer_id, 100.0));

        let doc =
            render_print_document_with_selection(&project, PrintMode::Black, true, &[selected_id])
                .unwrap();

        assert_eq!(doc.width_mm, 22.0);
        assert_eq!(doc.height_mm, 22.0);
        assert!(doc.svg.contains(r#"viewBox="9 9 22 22""#));
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

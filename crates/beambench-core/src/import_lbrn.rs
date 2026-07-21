//! Lbrn project (`.lbrn2` and legacy `.lbrn`) import.
//!
//! Lbrn does not publish a formal project-file schema, so this parser is
//! deliberately tolerant: it reads the stable XML concepts shared by both
//! formats, preserves known artwork and cut settings, and reports unknown
//! elements without rejecting the rest of the project.

use std::collections::HashMap;

use base64::Engine;
use beambench_common::path::{PathCommand, SubPath, VecPath};
use beambench_common::{RasterAdjustments, RasterMode, Transform2D};
use roxmltree::Node;

use crate::OperationType;

// The external XML format uses a product-specific root element. Keep the
// compatibility token out of user-facing names while still accepting genuine
// `.lbrn` and `.lbrn2` files.
const LBRN_PROJECT_ROOT: &str = concat!("Light", "BurnProject");

#[derive(Debug, Clone, PartialEq)]
pub struct LbrnDocument {
    pub app_version: String,
    pub format_version: String,
    pub material_height_mm: Option<f64>,
    pub notes: String,
    pub layers: Vec<LbrnLayer>,
    pub shapes: Vec<LbrnShape>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LbrnLayer {
    pub index: u32,
    pub name: String,
    pub priority: i32,
    pub entries: Vec<LbrnCutEntry>,
    pub is_tool_layer: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LbrnCutEntry {
    pub operation: OperationType,
    pub speed_mm_min: f64,
    pub power_percent: f64,
    pub power_min_percent: f64,
    pub passes: u32,
    pub air_assist: bool,
    pub output_enabled: bool,
    pub line_interval_mm: Option<f64>,
    pub scan_angle_deg: Option<f64>,
    pub crosshatch: bool,
    pub raster_mode: Option<RasterMode>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LbrnShape {
    Rectangle {
        layer_index: u32,
        transform: Transform2D,
        width_mm: f64,
        height_mm: f64,
        corner_radius_mm: f64,
    },
    Ellipse {
        layer_index: u32,
        transform: Transform2D,
        radius_x_mm: f64,
        radius_y_mm: f64,
    },
    Path {
        layer_index: u32,
        path: VecPath,
    },
    Text {
        layer_index: u32,
        transform: Transform2D,
        content: String,
        font_family: String,
        font_height_mm: f64,
        horizontal_alignment: i32,
        vertical_alignment: i32,
        bold: bool,
        italic: bool,
        welded: bool,
        letter_spacing_mm: f64,
        line_spacing_mm: f64,
    },
    Bitmap {
        layer_index: u32,
        transform: Transform2D,
        width_mm: f64,
        height_mm: f64,
        filename: String,
        data: Vec<u8>,
        adjustments: RasterAdjustments,
    },
    Group {
        children: Vec<LbrnShape>,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Vertex {
    x: f64,
    y: f64,
    outgoing: Option<(f64, f64)>,
    incoming: Option<(f64, f64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrimitiveKind {
    Line,
    Bezier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Primitive {
    kind: PrimitiveKind,
    from: usize,
    to: usize,
}

/// Parse either a Lbrn 2 (`FormatVersion="1"`) or legacy Lbrn
/// (`FormatVersion="0"`) project from XML bytes.
pub fn parse_lbrn_project(bytes: &[u8]) -> Result<LbrnDocument, String> {
    let xml = std::str::from_utf8(bytes)
        .map_err(|e| format!("Lbrn project is not valid UTF-8 XML: {e}"))?;
    let document = roxmltree::Document::parse(xml)
        .map_err(|e| format!("Lbrn project XML could not be parsed: {e}"))?;
    let root = document.root_element();
    if root.tag_name().name() != LBRN_PROJECT_ROOT {
        return Err("File is not a Lbrn project".to_string());
    }

    let app_version = root
        .attribute("AppVersion")
        .unwrap_or("unknown")
        .to_string();
    let format_version = root
        .attribute("FormatVersion")
        .unwrap_or("unknown")
        .to_string();
    let material_height_mm = parse_attr_f64(root, "MaterialHeight").filter(|v| *v > 0.0);
    let notes = root
        .children()
        .find(|node| node.has_tag_name("Notes"))
        .and_then(|node| node.attribute("Notes"))
        .unwrap_or("")
        .to_string();

    let mut layers = root
        .children()
        .filter(|node| {
            let name = node.tag_name().name();
            name == "CutSetting" || name.starts_with("CutSetting_")
        })
        .filter_map(parse_cut_layer)
        .collect::<Vec<_>>();
    layers.sort_by_key(|layer| (layer.priority, layer.index));

    let shared_paths = collect_shared_paths(root);
    let shared_bitmaps = collect_shared_bitmaps(root);
    let mut warnings = Vec::new();
    let mut shapes = Vec::new();
    for node in root.children().filter(|node| node.has_tag_name("Shape")) {
        if let Some(shape) = parse_shape(
            node,
            Transform2D::identity(),
            &shared_paths,
            &shared_bitmaps,
            &mut warnings,
        ) {
            shapes.push(shape);
        }
    }

    if shapes.is_empty() {
        return Err("Lbrn project contains no supported artwork".to_string());
    }

    Ok(LbrnDocument {
        app_version,
        format_version,
        material_height_mm,
        notes,
        layers,
        shapes,
        warnings,
    })
}

fn parse_cut_layer(node: Node<'_, '_>) -> Option<LbrnLayer> {
    let index = child_value(node, &["index"])?.parse::<u32>().ok()?;
    let name = child_value(node, &["name"])
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| lbrn_layer_name(index));
    let priority = child_value(node, &["priority"])
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(index as i32);
    let is_tool_layer =
        index >= 30 || name.eq_ignore_ascii_case("T1") || name.eq_ignore_ascii_case("T2");

    if is_tool_layer {
        return Some(LbrnLayer {
            index,
            name,
            priority,
            entries: Vec::new(),
            is_tool_layer: true,
        });
    }

    let raw_type = node.attribute("type").unwrap_or_else(|| {
        if node.tag_name().name() == "CutSetting_Img" {
            "Image"
        } else {
            "Cut"
        }
    });
    let operations = lbrn_operations(raw_type, node.tag_name().name());
    let speed_mm_min = child_value(node, &["speed"])
        .and_then(|value| value.parse::<f64>().ok())
        // Lbrn stores speed in mm/s even when its UI displays mm/min.
        .map(|speed_mm_s| speed_mm_s * 60.0)
        .unwrap_or(1000.0);
    let power_percent = child_value(node, &["maxPower", "power"])
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(50.0)
        .clamp(0.0, 100.0);
    let power_min_percent = child_value(node, &["minPower"])
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0)
        .clamp(0.0, 100.0);
    let passes = child_value(node, &["numPasses", "passes", "passCount"])
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1)
        .max(1);
    let air_assist = child_bool(node, &["airAssist", "air"]);
    let output_enabled = child_value(node, &["output", "outputEnabled"])
        .map(parse_lbrn_bool)
        .unwrap_or(true);
    let line_interval_mm = child_value(node, &["interval", "lineInterval"])
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| *value > 0.0);
    let scan_angle_deg =
        child_value(node, &["angle", "scanAngle"]).and_then(|value| value.parse::<f64>().ok());
    let crosshatch = child_bool(node, &["crossHatch", "crosshatch"]);
    let raster_mode = child_value(node, &["ditherMode", "imageMode"]).map(parse_raster_mode);

    let entries = operations
        .into_iter()
        .map(|operation| LbrnCutEntry {
            operation,
            speed_mm_min,
            power_percent,
            power_min_percent,
            passes,
            air_assist,
            output_enabled,
            line_interval_mm,
            scan_angle_deg,
            crosshatch,
            raster_mode,
        })
        .collect();

    Some(LbrnLayer {
        index,
        name,
        priority,
        entries,
        is_tool_layer: false,
    })
}

fn lbrn_operations(raw_type: &str, tag_name: &str) -> Vec<OperationType> {
    let normalized = raw_type
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if tag_name == "CutSetting_Img" || normalized == "image" {
        vec![OperationType::Image]
    } else if normalized.contains("cutscan") || normalized.contains("scanandcut") {
        vec![OperationType::Fill, OperationType::Line]
    } else if normalized.contains("offset") {
        vec![OperationType::OffsetFill]
    } else if normalized.contains("scan") || normalized.contains("fill") {
        vec![OperationType::Fill]
    } else {
        // Lbrn calls its vector-line operation "Cut" internally.
        vec![OperationType::Line]
    }
}

fn parse_raster_mode(value: &str) -> RasterMode {
    let normalized = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    match normalized.as_str() {
        "grayscale" | "greyscale" => RasterMode::Grayscale,
        "threshold" | "passthrough" => RasterMode::Threshold,
        "floydsteinberg" => RasterMode::FloydSteinberg,
        "ordered" | "ordereddither" => RasterMode::OrderedDither,
        "jarvis" => RasterMode::Jarvis,
        "sierra" => RasterMode::Sierra,
        "atkinson" => RasterMode::Atkinson,
        "halftone" => RasterMode::Halftone,
        "newsprint" => RasterMode::Newsprint,
        "sketch" => RasterMode::Sketch,
        _ => RasterMode::Stucki,
    }
}

fn parse_shape(
    node: Node<'_, '_>,
    parent_transform: Transform2D,
    shared_paths: &HashMap<(String, String), VecPath>,
    shared_bitmaps: &HashMap<(String, String), Vec<u8>>,
    warnings: &mut Vec<String>,
) -> Option<LbrnShape> {
    let shape_type = node.attribute("Type").unwrap_or("Unknown");
    let local_transform = parse_xform(node);
    let transform = parent_transform.compose(&local_transform);
    let layer_index = parse_attr_u32(node, "CutIndex").unwrap_or(0);

    match shape_type {
        "Rect" | "Rectangle" => Some(LbrnShape::Rectangle {
            layer_index,
            transform,
            width_mm: parse_attr_f64(node, "W").unwrap_or(0.0).abs(),
            height_mm: parse_attr_f64(node, "H").unwrap_or(0.0).abs(),
            corner_radius_mm: parse_attr_f64(node, "Cr").unwrap_or(0.0),
        }),
        "Ellipse" => Some(LbrnShape::Ellipse {
            layer_index,
            transform,
            radius_x_mm: parse_attr_f64(node, "Rx").unwrap_or(0.0).abs(),
            radius_y_mm: parse_attr_f64(node, "Ry").unwrap_or(0.0).abs(),
        }),
        "Path" => match parse_path(node, transform, shared_paths) {
            Ok(path) if !path.is_empty() => Some(LbrnShape::Path { layer_index, path }),
            Ok(_) => {
                warnings.push("Skipped an empty Lbrn path".to_string());
                None
            }
            Err(error) => {
                warnings.push(format!("Skipped a Lbrn path: {error}"));
                None
            }
        },
        "Text" => {
            let content = node.attribute("Str").unwrap_or("").to_string();
            if content.is_empty() {
                warnings.push("Skipped an empty Lbrn text object".to_string());
                return None;
            }
            let font = node.attribute("Font").unwrap_or("Arial");
            let font_parts = font.split(',').collect::<Vec<_>>();
            let weight = font_parts
                .get(4)
                .and_then(|value| value.parse::<i32>().ok())
                .unwrap_or(50);
            let italic = font_parts
                .get(5)
                .is_some_and(|value| parse_lbrn_bool(value));
            Some(LbrnShape::Text {
                layer_index,
                transform,
                content,
                font_family: font_parts.first().copied().unwrap_or("Arial").to_string(),
                font_height_mm: parse_attr_f64(node, "H").unwrap_or(5.0).abs().max(0.1),
                horizontal_alignment: parse_attr_i32(node, "Ah").unwrap_or(0),
                vertical_alignment: parse_attr_i32(node, "Av").unwrap_or(0),
                bold: weight >= 75,
                italic,
                welded: node.attribute("Weld").is_some_and(parse_lbrn_bool),
                letter_spacing_mm: parse_attr_f64(node, "LS").unwrap_or(0.0),
                line_spacing_mm: parse_attr_f64(node, "LnS").unwrap_or(0.0),
            })
        }
        "Bitmap" => {
            let encoded = node.attribute("Data").unwrap_or("").trim();
            let data = if encoded.is_empty() {
                bitmap_data_key(node)
                    .and_then(|key| shared_bitmaps.get(&key))
                    .cloned()
                    .or_else(|| {
                        warnings.push(format!(
                            "Skipped bitmap {} because it has no embedded image data",
                            node.attribute("File").unwrap_or("image")
                        ));
                        None
                    })?
            } else {
                match base64::engine::general_purpose::STANDARD.decode(encoded) {
                    Ok(data) => data,
                    Err(error) => {
                        warnings.push(format!("Skipped a Lbrn bitmap with invalid data: {error}"));
                        return None;
                    }
                }
            };
            let mut adjustments = RasterAdjustments::default();
            adjustments.gamma = parse_attr_f64(node, "Gamma").unwrap_or(1.0).clamp(0.1, 3.0);
            adjustments.contrast = normalize_percent_adjustment(parse_attr_f64(node, "Contrast"));
            adjustments.brightness =
                normalize_percent_adjustment(parse_attr_f64(node, "Brightness"));
            adjustments.enhance_amount = parse_attr_f64(node, "EnhanceAmount")
                .unwrap_or(0.0)
                .max(0.0);
            adjustments.enhance_radius = parse_attr_f64(node, "EnhanceRadius")
                .unwrap_or(0.0)
                .max(0.0);
            adjustments.enhance_denoise = parse_attr_f64(node, "EnhanceDenoise")
                .unwrap_or(0.0)
                .max(0.0);
            let filename = node
                .attribute("File")
                .and_then(|path| std::path::Path::new(path).file_name())
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .unwrap_or("Lbrn Bitmap.png")
                .to_string();
            Some(LbrnShape::Bitmap {
                layer_index,
                transform,
                width_mm: parse_attr_f64(node, "W").unwrap_or(0.0).abs(),
                height_mm: parse_attr_f64(node, "H").unwrap_or(0.0).abs(),
                filename,
                data,
                adjustments,
            })
        }
        "Group" => {
            let container = node
                .children()
                .find(|child| child.has_tag_name("Children"))
                .unwrap_or(node);
            let children = container
                .children()
                .filter(|child| child.has_tag_name("Shape"))
                .filter_map(|child| {
                    parse_shape(child, transform, shared_paths, shared_bitmaps, warnings)
                })
                .collect::<Vec<_>>();
            if children.is_empty() {
                warnings.push("Skipped an empty Lbrn group".to_string());
                None
            } else if children.len() == 1 {
                children.into_iter().next()
            } else {
                Some(LbrnShape::Group { children })
            }
        }
        other => {
            // Some Lbrn features retain an ordinary BackupPath. Prefer
            // that geometry over discarding the object outright.
            if let Some(backup) = node
                .children()
                .find(|child| child.has_tag_name("BackupPath"))
            {
                match parse_path(backup, transform, shared_paths) {
                    Ok(path) if !path.is_empty() => {
                        warnings.push(format!(
                            "Imported unsupported Lbrn {other} artwork from its backup path"
                        ));
                        return Some(LbrnShape::Path { layer_index, path });
                    }
                    _ => {}
                }
            }
            warnings.push(format!("Skipped unsupported Lbrn shape type: {other}"));
            None
        }
    }
}

fn bitmap_data_key(node: Node<'_, '_>) -> Option<(String, String)> {
    Some((
        node.attribute("File")?.to_string(),
        node.attribute("SourceHash").unwrap_or("").to_string(),
    ))
}

fn collect_shared_bitmaps(root: Node<'_, '_>) -> HashMap<(String, String), Vec<u8>> {
    let mut shared = HashMap::new();
    for node in root
        .descendants()
        .filter(|node| node.attribute("Type") == Some("Bitmap"))
    {
        let Some(key) = bitmap_data_key(node) else {
            continue;
        };
        let Some(encoded) = node
            .attribute("Data")
            .map(str::trim)
            .filter(|data| !data.is_empty())
        else {
            continue;
        };
        if let Ok(data) = base64::engine::general_purpose::STANDARD.decode(encoded) {
            shared.entry(key).or_insert(data);
        }
    }
    shared
}

fn normalize_percent_adjustment(value: Option<f64>) -> f64 {
    let value = value.unwrap_or(0.0);
    if value.abs() > 1.0 {
        (value / 100.0).clamp(-1.0, 1.0)
    } else {
        value.clamp(-1.0, 1.0)
    }
}

fn shared_path_key(node: Node<'_, '_>) -> Option<(String, String)> {
    Some((
        node.attribute("VertID")?.to_string(),
        node.attribute("PrimID")?.to_string(),
    ))
}

fn has_inline_vertices(node: Node<'_, '_>) -> bool {
    node.children()
        .any(|child| child.has_tag_name("V") || child.has_tag_name("VertList"))
}

fn collect_shared_paths(root: Node<'_, '_>) -> HashMap<(String, String), VecPath> {
    let mut shared = HashMap::new();
    for node in root
        .descendants()
        .filter(|node| node.attribute("Type") == Some("Path") && has_inline_vertices(*node))
    {
        let Some(key) = shared_path_key(node) else {
            continue;
        };
        if let Ok(path) = parse_inline_path(node, Transform2D::identity()) {
            shared.entry(key).or_insert(path);
        }
    }
    shared
}

fn parse_path(
    node: Node<'_, '_>,
    transform: Transform2D,
    shared_paths: &HashMap<(String, String), VecPath>,
) -> Result<VecPath, String> {
    if has_inline_vertices(node) {
        return parse_inline_path(node, transform);
    }
    if let Some(key) = shared_path_key(node) {
        let path = shared_paths.get(&key).ok_or_else(|| {
            format!(
                "path references missing shared geometry {} / {}",
                key.0, key.1
            )
        })?;
        return Ok(transform_path(path, transform));
    }
    parse_inline_path(node, transform)
}

fn parse_inline_path(node: Node<'_, '_>, transform: Transform2D) -> Result<VecPath, String> {
    let vertices = parse_vertices(node)?;
    if vertices.is_empty() {
        return Err("path has no vertices".to_string());
    }

    if let Some(prim_list) = node
        .children()
        .find(|child| child.has_tag_name("PrimList"))
        .and_then(|child| child.text())
    {
        let trimmed = prim_list.trim();
        if trimmed.eq_ignore_ascii_case("LineClosed") {
            return Ok(line_path(&vertices, transform, true));
        }
        if trimmed.eq_ignore_ascii_case("LineOpen") {
            return Ok(line_path(&vertices, transform, false));
        }
    }

    let primitives = parse_primitives(node)?;
    if primitives.is_empty() {
        return Ok(line_path(&vertices, transform, false));
    }

    build_primitive_path(&vertices, &primitives, transform)
}

fn transform_path(path: &VecPath, transform: Transform2D) -> VecPath {
    let subpaths = path
        .subpaths
        .iter()
        .map(|source| {
            let commands = source
                .commands
                .iter()
                .map(|command| match *command {
                    PathCommand::MoveTo { x, y } => {
                        let (x, y) = transformed_point(transform, x, y);
                        PathCommand::MoveTo { x, y }
                    }
                    PathCommand::LineTo { x, y } => {
                        let (x, y) = transformed_point(transform, x, y);
                        PathCommand::LineTo { x, y }
                    }
                    PathCommand::QuadTo { cx, cy, x, y } => {
                        let (cx, cy) = transformed_point(transform, cx, cy);
                        let (x, y) = transformed_point(transform, x, y);
                        PathCommand::QuadTo { cx, cy, x, y }
                    }
                    PathCommand::CubicTo {
                        c1x,
                        c1y,
                        c2x,
                        c2y,
                        x,
                        y,
                    } => {
                        let (c1x, c1y) = transformed_point(transform, c1x, c1y);
                        let (c2x, c2y) = transformed_point(transform, c2x, c2y);
                        let (x, y) = transformed_point(transform, x, y);
                        PathCommand::CubicTo {
                            c1x,
                            c1y,
                            c2x,
                            c2y,
                            x,
                            y,
                        }
                    }
                    PathCommand::Close => PathCommand::Close,
                })
                .collect();
            SubPath {
                commands,
                closed: source.closed,
            }
        })
        .collect();
    VecPath { subpaths }
}

fn parse_vertices(node: Node<'_, '_>) -> Result<Vec<Vertex>, String> {
    let legacy = node
        .children()
        .filter(|child| child.has_tag_name("V"))
        .map(|child| {
            let x = parse_attr_f64(child, "vx").unwrap_or(0.0);
            let y = parse_attr_f64(child, "vy").unwrap_or(0.0);
            let outgoing = match (parse_attr_f64(child, "c0x"), parse_attr_f64(child, "c0y")) {
                (Some(x), Some(y)) => Some((x, y)),
                _ => None,
            };
            let incoming = match (parse_attr_f64(child, "c1x"), parse_attr_f64(child, "c1y")) {
                (Some(x), Some(y)) => Some((x, y)),
                _ => None,
            };
            Vertex {
                x,
                y,
                outgoing,
                incoming,
            }
        })
        .collect::<Vec<_>>();
    if !legacy.is_empty() {
        return Ok(legacy);
    }

    let compact = node
        .children()
        .find(|child| child.has_tag_name("VertList"))
        .and_then(|child| child.text())
        .unwrap_or("");
    let starts = compact
        .char_indices()
        .filter_map(|(index, ch)| (ch == 'V').then_some(index))
        .collect::<Vec<_>>();
    let mut vertices = Vec::with_capacity(starts.len());
    for (position, start) in starts.iter().copied().enumerate() {
        let end = starts.get(position + 1).copied().unwrap_or(compact.len());
        let segment = &compact[start + 1..end];
        let controls_start = segment.find('c').unwrap_or(segment.len());
        let coords = segment[..controls_start]
            .split_whitespace()
            .collect::<Vec<_>>();
        if coords.len() < 2 {
            return Err("invalid compact vertex list".to_string());
        }
        let x = coords[0]
            .parse::<f64>()
            .map_err(|_| "invalid compact vertex x coordinate".to_string())?;
        let y = coords[1]
            .parse::<f64>()
            .map_err(|_| "invalid compact vertex y coordinate".to_string())?;
        let controls = &segment[controls_start..];
        let outgoing = compact_control(controls, "c0x", "c0y");
        let incoming = compact_control(controls, "c1x", "c1y");
        vertices.push(Vertex {
            x,
            y,
            outgoing,
            incoming,
        });
    }
    Ok(vertices)
}

fn compact_control(segment: &str, x_key: &str, y_key: &str) -> Option<(f64, f64)> {
    let x = compact_marker_value(segment, x_key)?;
    let y = compact_marker_value(segment, y_key)?;
    Some((x, y))
}

fn compact_marker_value(segment: &str, key: &str) -> Option<f64> {
    let start = segment.find(key)? + key.len();
    let rest = &segment[start..];
    let end = rest.find('c').unwrap_or(rest.len());
    rest[..end].trim().parse::<f64>().ok()
}

fn parse_primitives(node: Node<'_, '_>) -> Result<Vec<Primitive>, String> {
    let legacy = node
        .children()
        .filter(|child| child.has_tag_name("P"))
        .filter_map(|child| {
            let kind = match child.attribute("T").unwrap_or("L") {
                "B" => PrimitiveKind::Bezier,
                _ => PrimitiveKind::Line,
            };
            Some(Primitive {
                kind,
                from: child.attribute("p0")?.parse().ok()?,
                to: child.attribute("p1")?.parse().ok()?,
            })
        })
        .collect::<Vec<_>>();
    if !legacy.is_empty() {
        return Ok(legacy);
    }

    let compact = node
        .children()
        .find(|child| child.has_tag_name("PrimList"))
        .and_then(|child| child.text())
        .unwrap_or("")
        .trim();
    let mut primitives = Vec::new();
    let mut cursor = 0;
    let bytes = compact.as_bytes();
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }
        let kind = match bytes[cursor] {
            b'B' => PrimitiveKind::Bezier,
            b'L' => PrimitiveKind::Line,
            _ => return Err("invalid compact primitive list".to_string()),
        };
        cursor += 1;
        let next = compact[cursor..]
            .find(['B', 'L'])
            .map(|offset| cursor + offset)
            .unwrap_or(bytes.len());
        let indices = compact[cursor..next].split_whitespace().collect::<Vec<_>>();
        if indices.len() != 2 {
            return Err("invalid compact primitive indices".to_string());
        }
        primitives.push(Primitive {
            kind,
            from: indices[0]
                .parse()
                .map_err(|_| "invalid primitive start index".to_string())?,
            to: indices[1]
                .parse()
                .map_err(|_| "invalid primitive end index".to_string())?,
        });
        cursor = next;
    }
    Ok(primitives)
}

fn line_path(vertices: &[Vertex], transform: Transform2D, closed: bool) -> VecPath {
    let mut subpath = SubPath::new();
    let first = transform.apply(&beambench_common::Point2D::new(
        vertices[0].x,
        vertices[0].y,
    ));
    subpath.commands.push(PathCommand::MoveTo {
        x: first.x,
        y: first.y,
    });
    for vertex in &vertices[1..] {
        let point = transform.apply(&beambench_common::Point2D::new(vertex.x, vertex.y));
        subpath.commands.push(PathCommand::LineTo {
            x: point.x,
            y: point.y,
        });
    }
    if closed {
        subpath.commands.push(PathCommand::Close);
        subpath.closed = true;
    }
    VecPath {
        subpaths: vec![subpath],
    }
}

fn build_primitive_path(
    vertices: &[Vertex],
    primitives: &[Primitive],
    transform: Transform2D,
) -> Result<VecPath, String> {
    let mut subpaths = Vec::new();
    let mut current = SubPath::new();
    let mut current_index = None;
    let mut start_index = None;

    for primitive in primitives {
        let from = *vertices
            .get(primitive.from)
            .ok_or_else(|| "primitive references a missing start vertex".to_string())?;
        let to = *vertices
            .get(primitive.to)
            .ok_or_else(|| "primitive references a missing end vertex".to_string())?;

        if current_index != Some(primitive.from) {
            if !current.commands.is_empty() {
                subpaths.push(current);
                current = SubPath::new();
            }
            let point = transformed_point(transform, from.x, from.y);
            current.commands.push(PathCommand::MoveTo {
                x: point.0,
                y: point.1,
            });
            start_index = Some(primitive.from);
        }

        let endpoint = transformed_point(transform, to.x, to.y);
        match primitive.kind {
            PrimitiveKind::Line => current.commands.push(PathCommand::LineTo {
                x: endpoint.0,
                y: endpoint.1,
            }),
            PrimitiveKind::Bezier => {
                let outgoing = from.outgoing.unwrap_or((from.x, from.y));
                let incoming = to.incoming.unwrap_or((to.x, to.y));
                let c1 = transformed_point(transform, outgoing.0, outgoing.1);
                let c2 = transformed_point(transform, incoming.0, incoming.1);
                current.commands.push(PathCommand::CubicTo {
                    c1x: c1.0,
                    c1y: c1.1,
                    c2x: c2.0,
                    c2y: c2.1,
                    x: endpoint.0,
                    y: endpoint.1,
                });
            }
        }
        current_index = Some(primitive.to);

        if start_index == Some(primitive.to) {
            current.commands.push(PathCommand::Close);
            current.closed = true;
            subpaths.push(current);
            current = SubPath::new();
            current_index = None;
            start_index = None;
        }
    }

    if !current.commands.is_empty() {
        subpaths.push(current);
    }
    Ok(VecPath { subpaths })
}

fn transformed_point(transform: Transform2D, x: f64, y: f64) -> (f64, f64) {
    let point = transform.apply(&beambench_common::Point2D::new(x, y));
    (point.x, point.y)
}

fn parse_xform(node: Node<'_, '_>) -> Transform2D {
    let values = node
        .children()
        .find(|child| child.has_tag_name("XForm"))
        .and_then(|child| child.text())
        .map(|text| {
            text.split_whitespace()
                .filter_map(|value| value.parse::<f64>().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if values.len() == 6 {
        // Lbrn order: m11 m12 m21 m22 m31 m32.
        Transform2D {
            a: values[0],
            b: values[1],
            c: values[2],
            d: values[3],
            tx: values[4],
            ty: values[5],
        }
    } else {
        Transform2D::identity()
    }
}

fn child_value<'a>(node: Node<'a, 'a>, names: &[&str]) -> Option<&'a str> {
    node.children()
        .find(|child| {
            names
                .iter()
                .any(|name| child.tag_name().name().eq_ignore_ascii_case(name))
        })
        .and_then(|child| child.attribute("Value").or_else(|| child.text()))
}

fn child_bool(node: Node<'_, '_>, names: &[&str]) -> bool {
    child_value(node, names)
        .map(parse_lbrn_bool)
        .unwrap_or(false)
}

fn parse_lbrn_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on" | "-1"
    )
}

fn parse_attr_f64(node: Node<'_, '_>, name: &str) -> Option<f64> {
    node.attribute(name)?
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
}

fn parse_attr_u32(node: Node<'_, '_>, name: &str) -> Option<u32> {
    node.attribute(name)?.parse::<u32>().ok()
}

fn parse_attr_i32(node: Node<'_, '_>, name: &str) -> Option<i32> {
    node.attribute(name)?.parse::<i32>().ok()
}

fn lbrn_layer_name(index: u32) -> String {
    match index {
        30 => "T1".to_string(),
        31 => "T2".to_string(),
        _ => format!("C{index:02}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<LBRN_PROJECT_ROOT AppVersion="1.6.03" FormatVersion="1" MaterialHeight="3" MirrorX="False" MirrorY="False">
  <CutSetting type="Cut"><index Value="1"/><name Value="C01"/><minPower Value="5"/><maxPower Value="20"/><speed Value="100"/><priority Value="1"/></CutSetting>
  <CutSetting_Img type="Image"><index Value="4"/><name Value="C04"/><maxPower Value="35"/><speed Value="50"/><priority Value="2"/><ditherMode Value="stucki"/></CutSetting_Img>
  <Shape Type="Group"><XForm>1 0 0 1 10 20</XForm><Children>
    <Shape Type="Rect" CutIndex="1" W="40" H="20" Cr="2"><XForm>1 0 0 1 30 40</XForm></Shape>
    <Shape Type="Path" CutIndex="1"><XForm>1 0 0 1 5 6</XForm><VertList>V0 0c0x1c1x1V10 0c0x1c1x1V10 10c0x1c1x1</VertList><PrimList>LineClosed</PrimList></Shape>
  </Children></Shape>
  <Shape Type="Text" CutIndex="1" Font="Arial,-1,100,5,75,1,0,0,0,0" Str="Beam Bench" H="12" LS="1" LnS="2" Ah="1" Av="1" Weld="1"><XForm>1 0 0 1 80 90</XForm></Shape>
  <Notes ShowOnLoad="0" Notes="Fixture notes"/>
</LBRN_PROJECT_ROOT>"#;

    fn project_xml(template: &str) -> String {
        template.replace("LBRN_PROJECT_ROOT", LBRN_PROJECT_ROOT)
    }

    #[test]
    fn parses_lbrn2_layers_shapes_and_settings() {
        let project = project_xml(PROJECT);
        let parsed = parse_lbrn_project(project.as_bytes()).unwrap();
        assert_eq!(parsed.app_version, "1.6.03");
        assert_eq!(parsed.format_version, "1");
        assert_eq!(parsed.material_height_mm, Some(3.0));
        assert_eq!(parsed.notes, "Fixture notes");
        assert_eq!(parsed.layers.len(), 2);
        assert_eq!(parsed.layers[0].entries[0].speed_mm_min, 6000.0);
        assert_eq!(parsed.layers[0].entries[0].power_min_percent, 5.0);
        assert_eq!(parsed.layers[1].entries[0].operation, OperationType::Image);
        assert_eq!(parsed.shapes.len(), 2);
        let LbrnShape::Group { children } = &parsed.shapes[0] else {
            panic!("expected group")
        };
        assert_eq!(children.len(), 2);
        let LbrnShape::Rectangle { transform, .. } = &children[0] else {
            panic!("expected rectangle")
        };
        assert_eq!(transform.tx, 40.0);
        assert_eq!(transform.ty, 60.0);
    }

    #[test]
    fn parses_legacy_vertices_and_bezier_primitives() {
        let xml = project_xml(
            r#"<LBRN_PROJECT_ROOT AppVersion="1.6.03" FormatVersion="0">
          <CutSetting type="Cut"><index Value="0"/><name Value="C00"/></CutSetting>
          <Shape Type="Path" CutIndex="0"><XForm>1 0 0 1 2 3</XForm>
            <V vx="0" vy="0" c0x="2" c0y="0"/>
            <V vx="10" vy="10" c1x="8" c1y="10"/>
            <P T="B" p0="0" p1="1"/>
          </Shape>
        </LBRN_PROJECT_ROOT>"#,
        );
        let parsed = parse_lbrn_project(xml.as_bytes()).unwrap();
        let LbrnShape::Path { path, .. } = &parsed.shapes[0] else {
            panic!("expected path")
        };
        assert_eq!(
            path.subpaths[0].commands[1],
            PathCommand::CubicTo {
                c1x: 4.0,
                c1y: 3.0,
                c2x: 10.0,
                c2y: 13.0,
                x: 12.0,
                y: 13.0,
            }
        );
    }

    #[test]
    fn skips_unknown_shapes_but_keeps_supported_artwork() {
        let xml = project_xml(
            r#"<LBRN_PROJECT_ROOT AppVersion="2.0" FormatVersion="2">
          <CutSetting type="Cut"><index Value="0"/></CutSetting>
          <Shape Type="FutureShape" CutIndex="0"/>
          <Shape Type="Ellipse" CutIndex="0" Rx="5" Ry="6"><XForm>1 0 0 1 10 20</XForm></Shape>
        </LBRN_PROJECT_ROOT>"#,
        );
        let parsed = parse_lbrn_project(xml.as_bytes()).unwrap();
        assert_eq!(parsed.shapes.len(), 1);
        assert_eq!(parsed.warnings.len(), 1);
    }

    #[test]
    fn resolves_reused_path_geometry_by_lbrn_ids() {
        let xml = project_xml(
            r#"<LBRN_PROJECT_ROOT AppVersion="1.6.03" FormatVersion="1">
          <CutSetting type="Cut"><index Value="0"/><name Value="C00"/></CutSetting>
          <Shape Type="Path" CutIndex="0" VertID="7" PrimID="9">
            <XForm>1 0 0 1 0 0</XForm>
            <VertList>V0 0c0x1c1x1V10 0c0x1c1x1V10 10c0x1c1x1</VertList>
            <PrimList>LineClosed</PrimList>
          </Shape>
          <Shape Type="Path" CutIndex="0" VertID="7" PrimID="9">
            <XForm>1 0 0 1 12 34</XForm>
          </Shape>
        </LBRN_PROJECT_ROOT>"#,
        );
        let parsed = parse_lbrn_project(xml.as_bytes()).unwrap();
        assert_eq!(parsed.shapes.len(), 2);
        assert!(parsed.warnings.is_empty());
        let LbrnShape::Path { path, .. } = &parsed.shapes[1] else {
            panic!("expected reused path")
        };
        assert_eq!(
            path.subpaths[0].commands[0],
            PathCommand::MoveTo { x: 12.0, y: 34.0 }
        );
    }

    #[test]
    fn resolves_reused_bitmap_data_by_source_identity() {
        let xml = project_xml(
            r#"<LBRN_PROJECT_ROOT AppVersion="1.6.03" FormatVersion="1">
          <CutSetting_Img type="Image"><index Value="0"/><name Value="C00"/></CutSetting_Img>
          <Shape Type="Bitmap" CutIndex="0" File="art.png" SourceHash="7" Data="AQID" W="10" H="20">
            <XForm>1 0 0 1 0 0</XForm>
          </Shape>
          <Shape Type="Bitmap" CutIndex="0" File="art.png" SourceHash="7" W="10" H="20">
            <XForm>1 0 0 1 30 40</XForm>
          </Shape>
        </LBRN_PROJECT_ROOT>"#,
        );
        let parsed = parse_lbrn_project(xml.as_bytes()).unwrap();
        assert_eq!(parsed.shapes.len(), 2);
        assert!(parsed.warnings.is_empty());
        let LbrnShape::Bitmap {
            data, transform, ..
        } = &parsed.shapes[1]
        else {
            panic!("expected reused bitmap")
        };
        assert_eq!(data, &[1, 2, 3]);
        assert_eq!(transform.tx, 30.0);
        assert_eq!(transform.ty, 40.0);
    }
}

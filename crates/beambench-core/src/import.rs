use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};
use thiserror::Error;
use usvg::tiny_skia_path::PathSegment;

use beambench_common::path::{PathCommand, SubPath, VecPath};
use beambench_common::{Bounds, ColorTag, Point2D, Transform2D};

use crate::asset::{Asset, AssetMediaType};
use crate::layer::{Layer, LayerId, OperationType};
use crate::object::{
    ObjectData, ObjectId, ProjectObject, TextAlignment, TextAlignmentV, TextLayoutMode,
};
use crate::project::Project;
use crate::vector::boolean::{
    normalize_subject_with_tolerance, path_intersection_with_tolerance, weld_shapes_with_tolerance,
};
use crate::vector::flatten::flatten_vecpath;
use crate::vector::normalize::{VECTOR_PATH_FLATTEN_TOLERANCE_MM, cleanup_vecpath_for_planner};
use crate::vector::text_to_path::{can_resolve_font, font_ascender_mm};

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("SVG parse error: {0}")]
    ParseError(String),
    #[error("image decode error: {0}")]
    ImageDecodeError(String),
}

pub const MAX_AUTO_COLOR_LAYERS: usize = 16;

/// Compare exact RGB colors without palette snapping. Ignore a valid alpha
/// suffix, since operation layers carry RGB colors rather than transparency.
pub fn normalize_import_color_tag(color_tag: &str) -> Option<String> {
    let hex = color_tag.trim().strip_prefix('#')?;
    (matches!(hex.len(), 6 | 8) && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| format!("#{}", hex[..6].to_ascii_uppercase()))
}

/// Reuse configured vector operations, or create an ordinary Line layer.
/// A source color never implies a cutting or engraving operation.
pub fn resolve_import_color_layer(project: &mut Project, color_tag: &str) -> Option<LayerId> {
    let normalized = normalize_import_color_tag(color_tag)?;
    if let Some(layer) = project.layers.iter().find(|layer| {
        !layer.is_tool_layer
            && !layer.entries.is_empty()
            && layer
                .entries
                .iter()
                .all(|entry| entry.operation != OperationType::Image && !entry.operation.is_tool())
            && normalize_import_color_tag(&layer.color_tag.0).as_deref() == Some(&normalized)
    }) {
        return Some(layer.id);
    }
    let mut layer = Layer::new(format!("Imported {normalized}"), OperationType::Line);
    layer.color_tag = ColorTag(normalized);
    let id = layer.id;
    project.add_layer(layer);
    Some(id)
}

pub fn should_auto_route_import_colors<'a>(
    colors: impl IntoIterator<Item = Option<&'a str>>,
) -> bool {
    let mut distinct = HashSet::new();
    for color in colors
        .into_iter()
        .flatten()
        .filter_map(normalize_import_color_tag)
    {
        distinct.insert(color);
        if distinct.len() > MAX_AUTO_COLOR_LAYERS {
            return false;
        }
    }
    !distinct.is_empty()
}

/// Metadata extracted from an SVG `<text>` element before usvg flattening.
struct SvgTextElement {
    color_tag: String,
    content: String,
    font_family: String,
    font_size_px: f64,
    x: f64,
    y: f64,
    bold: bool,
    italic: bool,
    alignment: TextAlignment,
    alignment_v: TextAlignmentV,
    h_spacing_px: f64,
    rtl: bool,
}

/// Parse an SVG/CSS `font-size` value into px. Supports `px`, `pt`, `pc`,
/// `mm`, `cm`, `in`, `em`/`rem` (assuming a 16px root), `%` (of 16px), and
/// bare numbers (treated as px). Unknown units fall back to 16px.
fn parse_svg_font_size(value: &str) -> f64 {
    const FALLBACK_PX: f64 = 16.0;
    let v = value.trim().to_ascii_lowercase();
    let (number, factor) = if let Some(n) = v.strip_suffix("px") {
        (n, 1.0)
    } else if let Some(n) = v.strip_suffix("pt") {
        (n, 96.0 / 72.0)
    } else if let Some(n) = v.strip_suffix("pc") {
        (n, 16.0)
    } else if let Some(n) = v.strip_suffix("mm") {
        (n, 96.0 / 25.4)
    } else if let Some(n) = v.strip_suffix("cm") {
        (n, 96.0 / 2.54)
    } else if let Some(n) = v.strip_suffix("in") {
        (n, 96.0)
    } else if let Some(n) = v.strip_suffix("rem") {
        (n, 16.0)
    } else if let Some(n) = v.strip_suffix("em") {
        (n, 16.0)
    } else if let Some(n) = v.strip_suffix('%') {
        (n, 16.0 / 100.0)
    } else {
        (v.as_str(), 1.0)
    };
    match number.trim().parse::<f64>() {
        Ok(n) if n.is_finite() => n * factor,
        _ => FALLBACK_PX,
    }
}

/// Extract a CSS property value from an inline `style` attribute string.
/// Returns the trimmed value or `None` if the property is not present.
fn style_property<'a>(style: Option<&'a str>, prop: &str) -> Option<&'a str> {
    style.and_then(|s| {
        s.split(';')
            .find(|p| p.trim().starts_with(prop))
            .and_then(|p| p.split(':').nth(1))
            .map(|v| v.trim())
    })
}

/// Map the root `viewBox` onto the document size in px, the space usvg
/// reports paths in, so text placed in user units lands with the paths.
fn root_viewbox_transform(root: roxmltree::Node) -> Transform2D {
    let identity = Transform2D::identity();
    let Some(view_box) = root
        .attribute("viewBox")
        .and_then(|value| value.parse::<svgtypes::ViewBox>().ok())
        .filter(|vb| vb.w > 0.0 && vb.h > 0.0)
    else {
        return identity;
    };
    let length_px = |name: &str| -> Option<f64> {
        let length = root.attribute(name)?.parse::<svgtypes::Length>().ok()?;
        let factor = match length.unit {
            svgtypes::LengthUnit::None | svgtypes::LengthUnit::Px => 1.0,
            svgtypes::LengthUnit::In => 96.0,
            svgtypes::LengthUnit::Cm => 96.0 / 2.54,
            svgtypes::LengthUnit::Mm => 96.0 / 25.4,
            svgtypes::LengthUnit::Pt => 96.0 / 72.0,
            svgtypes::LengthUnit::Pc => 16.0,
            _ => return None,
        };
        Some(length.number * factor).filter(|px| *px > 0.0)
    };
    let width = length_px("width").unwrap_or(view_box.w);
    let height = length_px("height").unwrap_or(view_box.h);
    let (sx, sy) = (width / view_box.w, height / view_box.h);
    let aspect = root
        .attribute("preserveAspectRatio")
        .and_then(|value| value.parse::<svgtypes::AspectRatio>().ok())
        .unwrap_or_default();
    use svgtypes::Align;
    let (fx, fy) = match aspect.align {
        Align::None => {
            return Transform2D {
                a: sx,
                b: 0.0,
                c: 0.0,
                d: sy,
                tx: -view_box.x * sx,
                ty: -view_box.y * sy,
            };
        }
        Align::XMinYMin => (0.0, 0.0),
        Align::XMidYMin => (0.5, 0.0),
        Align::XMaxYMin => (1.0, 0.0),
        Align::XMinYMid => (0.0, 0.5),
        Align::XMidYMid => (0.5, 0.5),
        Align::XMaxYMid => (1.0, 0.5),
        Align::XMinYMax => (0.0, 1.0),
        Align::XMidYMax => (0.5, 1.0),
        Align::XMaxYMax => (1.0, 1.0),
    };
    let s = if aspect.slice { sx.max(sy) } else { sx.min(sy) };
    Transform2D {
        a: s,
        b: 0.0,
        c: 0.0,
        d: s,
        tx: -view_box.x * s + (width - view_box.w * s) * fx,
        ty: -view_box.y * s + (height - view_box.h * s) * fy,
    }
}

/// The transform from a `<text>` element's user units to document px, when
/// the text is drawn directly with a uniform scale and no rotation or skew.
fn editable_text_transform(text: roxmltree::Node, root_map: &Transform2D) -> Option<Transform2D> {
    let root = text.document().root_element();
    let mut chain = Vec::new();
    for node in text.ancestors().filter(|node| node.is_element()) {
        let name = node.tag_name().name();
        if matches!(
            name,
            "defs" | "symbol" | "clipPath" | "mask" | "pattern" | "marker" | "switch"
        ) || (name == "svg" && node != root)
        {
            return None;
        }
        let style = node.attribute("style");
        let hidden = node
            .attribute("display")
            .or_else(|| style_property(style, "display"))
            == Some("none")
            || node
                .attribute("visibility")
                .or_else(|| style_property(style, "visibility"))
                .is_some_and(|value| value == "hidden" || value == "collapse")
            || node.attribute("clip-path").is_some()
            || style_property(style, "clip-path").is_some();
        if hidden {
            return None;
        }
        if let Some(value) = node.attribute("transform") {
            if node == root {
                return None;
            }
            let t = value.parse::<svgtypes::Transform>().ok()?;
            chain.push(Transform2D {
                a: t.a,
                b: t.b,
                c: t.c,
                d: t.d,
                tx: t.e,
                ty: t.f,
            });
        }
    }
    let mut map = *root_map;
    for t in chain.iter().rev() {
        map = map.compose(t);
    }
    let tolerance = 1e-9 * map.a.abs().max(1.0);
    let uniform = map.a > 0.0
        && (map.a - map.d).abs() <= tolerance
        && map.b.abs() <= tolerance
        && map.c.abs() <= tolerance;
    uniform.then_some(map)
}

/// Pre-parse SVG XML to extract `<text>` elements with their metadata.
/// Returns extracted text elements and modified SVG bytes with those `<text>` elements removed.
fn extract_svg_text_elements(
    svg_bytes: &[u8],
    text_colors: &HashMap<usize, String>,
) -> (Vec<SvgTextElement>, Vec<u8>) {
    let svg_str = match std::str::from_utf8(svg_bytes) {
        Ok(s) => s,
        Err(_) => return (vec![], svg_bytes.to_vec()),
    };

    let doc = match roxmltree::Document::parse(svg_str) {
        Ok(d) => d,
        Err(_) => return (vec![], svg_bytes.to_vec()),
    };

    let mut texts = Vec::new();
    let mut remove_ranges: Vec<std::ops::Range<usize>> = Vec::new();
    let root_map = root_viewbox_transform(doc.root_element());
    let reused_text: HashSet<_> = doc
        .descendants()
        .filter(|node| node.has_tag_name("use"))
        .flat_map(|node| node.attributes())
        .filter(|attribute| attribute.name() == "href")
        .filter_map(|attribute| attribute.value().strip_prefix('#'))
        .collect();

    for node in doc.descendants() {
        if node.tag_name().name() == "text" {
            // Removing a referenced label would also erase its <use> copies.
            // Let usvg outline the label and every transformed instance.
            if node.ancestors().any(|ancestor| {
                ancestor
                    .attribute("id")
                    .is_some_and(|id| reused_text.contains(id))
            }) {
                continue;
            }
            // Editable text keeps a position, size and spacing, but no
            // rotation or skew. Anything else stays in the SVG and imports as
            // exact outlines, as does text that is not drawn directly.
            let Some(user_to_document) = editable_text_transform(node, &root_map) else {
                continue;
            };
            // A single editable label cannot represent several span colors.
            // Keep those labels in usvg so their outlines retain each paint.
            let Some(color_tag) = text_colors.get(&node.range().start).cloned() else {
                continue;
            };
            // Extract text content (direct text children + <tspan> content)
            let content: String = node
                .descendants()
                .filter(|n| n.is_text())
                .filter_map(|n| n.text())
                .collect::<Vec<_>>()
                .join("")
                .trim()
                .to_string();

            if content.is_empty() {
                continue;
            }

            let style = node.attribute("style");

            let font_family = node
                .attribute("font-family")
                .or_else(|| {
                    style_property(style, "font-family")
                        .map(|v| v.trim_matches(|c: char| c == '\'' || c == '"'))
                })
                .unwrap_or("sans-serif")
                .trim_matches(|c: char| c == '\'' || c == '"')
                .to_string();

            let font_size_str = node
                .attribute("font-size")
                .or_else(|| style_property(style, "font-size"))
                .unwrap_or("16");
            let font_size_px: f64 = parse_svg_font_size(font_size_str);

            // Position: try parent <text> x/y first, fall back to first <tspan>
            let mut x: f64 = node
                .attribute("x")
                .and_then(|v| v.parse().ok())
                .unwrap_or(f64::NAN);
            let mut y: f64 = node
                .attribute("y")
                .and_then(|v| v.parse().ok())
                .unwrap_or(f64::NAN);
            if x.is_nan() || y.is_nan() {
                // Fall back to first <tspan> child position
                for child in node.children() {
                    if child.tag_name().name() == "tspan" {
                        if x.is_nan() {
                            x = child
                                .attribute("x")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0.0);
                        }
                        if y.is_nan() {
                            y = child
                                .attribute("y")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0.0);
                        }
                        break;
                    }
                }
                if x.is_nan() {
                    x = 0.0;
                }
                if y.is_nan() {
                    y = 0.0;
                }
            }

            let font_weight = node
                .attribute("font-weight")
                .or_else(|| style_property(style, "font-weight"))
                .unwrap_or("normal");
            let bold = font_weight == "bold" || font_weight == "700";

            let font_style_val = node
                .attribute("font-style")
                .or_else(|| style_property(style, "font-style"))
                .unwrap_or("normal");
            let italic = font_style_val == "italic" || font_style_val == "oblique";

            // text-anchor → alignment (attribute or style)
            let text_anchor = node
                .attribute("text-anchor")
                .or_else(|| style_property(style, "text-anchor"))
                .unwrap_or("start");
            let alignment = match text_anchor {
                "middle" => TextAlignment::Center,
                "end" => TextAlignment::Right,
                _ => TextAlignment::Left,
            };

            // dominant-baseline → alignment_v
            let alignment_v = match node.attribute("dominant-baseline").unwrap_or("auto") {
                "middle" | "central" => TextAlignmentV::Middle,
                "text-after-edge" | "ideographic" => TextAlignmentV::Bottom,
                _ => TextAlignmentV::Top,
            };

            // letter-spacing → h_spacing (attribute or style, in SVG px units)
            let letter_spacing_str = node
                .attribute("letter-spacing")
                .or_else(|| style_property(style, "letter-spacing"))
                .unwrap_or("0");
            let h_spacing_px: f64 = letter_spacing_str
                .trim_end_matches("px")
                .parse()
                .unwrap_or(0.0);

            // direction → rtl (attribute or style)
            let direction = node
                .attribute("direction")
                .or_else(|| style_property(style, "direction"))
                .unwrap_or("ltr");
            let rtl = direction == "rtl";

            // Check if we can resolve this font
            if can_resolve_font(&font_family, bold, italic) {
                let scale = user_to_document.a;
                let position = user_to_document.apply(&Point2D::new(x, y));
                texts.push(SvgTextElement {
                    color_tag,
                    content,
                    font_family,
                    font_size_px: font_size_px * scale,
                    x: position.x,
                    y: position.y,
                    bold,
                    italic,
                    alignment,
                    alignment_v,
                    h_spacing_px: h_spacing_px * scale,
                    rtl,
                });
                // Mark this element for removal from SVG
                let range = node.range();
                if !range.is_empty() {
                    remove_ranges.push(range);
                }
            }
        }
    }

    // Remove extracted text elements from SVG (in reverse order to preserve offsets)
    let mut modified = svg_str.to_string();
    remove_ranges.sort_by(|a, b| b.start.cmp(&a.start));
    for range in &remove_ranges {
        modified.replace_range(range.clone(), "");
    }

    (texts, modified.into_bytes())
}

fn ensure_svg_fallback_fonts(db: &mut fontdb::Database) {
    db.load_font_data(include_bytes!("../../fonts/LiberationSans-Regular.ttf").to_vec());
    // Preserve system generic families where they resolve. usvg uses Serif as
    // its final fallback, so both mappings must work on a fontless system.
    if db
        .query(&fontdb::Query {
            families: &[fontdb::Family::SansSerif],
            ..Default::default()
        })
        .is_none()
    {
        db.set_sans_serif_family("Liberation Sans");
    }
    if db
        .query(&fontdb::Query {
            families: &[fontdb::Family::Serif],
            ..Default::default()
        })
        .is_none()
    {
        db.set_serif_family("Liberation Sans");
    }
}

/// usvg only accepts UTF-8. Decode XML before both editable text extraction
/// and geometry parsing, without guessing an undeclared legacy encoding or
/// replacing invalid characters in artwork.
fn decode_svg_xml(bytes: &[u8]) -> Result<Cow<'_, str>, ImportError> {
    let (encoding, bom_len) = if let Some(detected) = Encoding::for_bom(bytes) {
        detected
    } else if bytes.starts_with(b"<\0?\0") {
        (UTF_16LE, 0)
    } else if bytes.starts_with(b"\0<\0?") {
        (UTF_16BE, 0)
    } else {
        let mut encoding = UTF_8;
        // Only inspect the XML declaration, never a comment, stylesheet PI
        // or an attribute in the artwork itself.
        if bytes.starts_with(b"<?xml") {
            let mut reader = quick_xml::Reader::from_reader(bytes);
            if let quick_xml::events::Event::Decl(declaration) = reader
                .read_event()
                .map_err(|e| ImportError::ParseError(e.to_string()))?
                && let Some(label) = declaration.encoding()
            {
                let label = label.map_err(|e| ImportError::ParseError(e.to_string()))?;
                encoding = Encoding::for_label_no_replacement(&label)
                    .filter(|encoding| encoding.is_ascii_compatible())
                    .ok_or_else(|| {
                        ImportError::ParseError(format!(
                            "Unsupported SVG text encoding '{}'. Save the SVG as UTF-8 and try again.",
                            String::from_utf8_lossy(&label)
                        ))
                    })?;
            }
        }
        (encoding, 0)
    };
    encoding
        .decode_without_bom_handling_and_without_replacement(&bytes[bom_len..])
        .ok_or_else(|| {
            ImportError::ParseError(format!(
                "Invalid {} SVG text. Save the SVG as UTF-8 and try again.",
                encoding.name()
            ))
        })
}

/// Import an SVG file into the project.
///
/// First pre-parses XML to extract `<text>` elements. If a text element's font
/// is available on the system, it creates an editable `ObjectData::Text` object.
/// Otherwise, usvg handles the text element (flattening to paths).
///
/// Remaining paths are grouped by paint, with coordinates transformed to
/// bed-space at true physical size, centered, on the requested layer.
pub fn import_svg(
    svg_bytes: &[u8],
    project: &mut Project,
    layer_id: LayerId,
) -> Result<Vec<ObjectId>, ImportError> {
    import_svg_impl(svg_bytes, project, layer_id, false)
}

/// Desktop imports share PDF/AI color routing. The explicit-layer importer
/// remains available to design commands and other callers choosing operations.
pub fn import_svg_with_color_layers(
    svg_bytes: &[u8],
    project: &mut Project,
    layer_id: LayerId,
) -> Result<Vec<ObjectId>, ImportError> {
    import_svg_impl(svg_bytes, project, layer_id, true)
}

/// Let usvg resolve CSS, inherited paints and currentColor for editable text.
/// Temporary IDs identify anonymous labels without changing the source SVG.
fn svg_text_colors(svg: &str, options: &mut usvg::Options) -> HashMap<usize, String> {
    let Ok(doc) = roxmltree::Document::parse(svg) else {
        return HashMap::new();
    };
    let mut prefix = "beambench-import-text-".to_string();
    while svg.contains(&prefix) {
        prefix.push('_');
    }
    let mut inserts = Vec::new();
    let mut ids = Vec::new();
    for node in doc.descendants().filter(|node| node.has_tag_name("text")) {
        let start = node.range().start;
        let id = node.attribute("id").map(str::to_string).unwrap_or_else(|| {
            let id = format!("{prefix}{start}");
            let tag_end = svg[start..]
                .find(|ch: char| ch.is_whitespace() || ch == '/' || ch == '>')
                .unwrap();
            inserts.push((start + tag_end, format!(" id=\"{id}\"")));
            id
        });
        ids.push((start, id));
    }
    if ids.is_empty() {
        return HashMap::new();
    }
    let mut identified = svg.to_string();
    for (offset, attribute) in inserts.into_iter().rev() {
        identified.insert_str(offset, &attribute);
    }
    options.fontdb_mut().load_system_fonts();
    ensure_svg_fallback_fonts(options.fontdb_mut());
    // Assigning an ID can change attribute-selector CSS such as text[id].
    // Keep those labels as outlines resolved from the untouched source.
    if doc
        .descendants()
        .filter(|node| node.has_tag_name("style"))
        .any(|node| node.text().is_some_and(|css| css.contains('[')))
    {
        return HashMap::new();
    }
    let Ok(tree) = usvg::Tree::from_str(&identified, options) else {
        return HashMap::new();
    };
    ids.into_iter()
        .filter_map(|(offset, id)| {
            resolved_text_color(tree.node_by_id(&id)?).map(|color| (offset, color))
        })
        .collect()
}

fn resolved_text_color(node: &usvg::Node) -> Option<String> {
    match node {
        usvg::Node::Text(text) => {
            let mut colors = text
                .chunks()
                .iter()
                .flat_map(|chunk| chunk.spans())
                .map(|span| {
                    if !span.is_visible() {
                        return None;
                    }
                    [
                        paint_hex(span.stroke().map(|stroke| stroke.paint())),
                        paint_hex(span.fill().map(|fill| fill.paint())),
                    ]
                    .into_iter()
                    .flatten()
                    .find(|color| color.starts_with('#'))
                });
            let first = colors.next()??;
            colors
                .all(|color| color.as_ref() == Some(&first))
                .then_some(first)
        }
        usvg::Node::Group(group) => group.children().iter().find_map(resolved_text_color),
        _ => None,
    }
}

fn import_svg_impl(
    svg_bytes: &[u8],
    project: &mut Project,
    layer_id: LayerId,
    route_colors: bool,
) -> Result<Vec<ObjectId>, ImportError> {
    let mut created_ids = Vec::new();

    let svg_bytes = if svg_bytes.starts_with(&[0x1f, 0x8b]) {
        Cow::Owned(
            usvg::decompress_svgz(svg_bytes).map_err(|e| ImportError::ParseError(e.to_string()))?,
        )
    } else {
        Cow::Borrowed(svg_bytes)
    };
    let svg_text = decode_svg_xml(&svg_bytes)?;

    let mut options = usvg::Options {
        // Only embedded data: an imported SVG must not pull in arbitrary
        // files from this computer through `<image href="/path">`.
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    };
    let text_colors = svg_text_colors(&svg_text, &mut options);
    let (text_elements, remaining_svg) =
        extract_svg_text_elements(svg_text.as_bytes(), &text_colors);
    let tree = usvg::Tree::from_data(&remaining_svg, &options)
        .map_err(|e| ImportError::ParseError(e.to_string()))?;

    // Convert to physical size and center on the bed. The design's true
    // measurements are NEVER scaled: laser work is dimension-accurate, so an
    // SVG larger than the workspace imports at its real size (overhanging the
    // bed on canvas) and the import pipeline surfaces an oversize warning.
    // The pre-job bounds check protects the actual cut.
    let svg_size = tree.size();
    let svg_w = svg_size.width() as f64;
    let svg_h = svg_size.height() as f64;
    let bed_w = project.workspace.bed_width_mm;
    let bed_h = project.workspace.bed_height_mm;
    // SVG user units are CSS pixels at 96 DPI; convert to mm for true physical size.
    const PX_TO_MM: f64 = 25.4 / 96.0;
    let scale = PX_TO_MM;
    let offset_x = (bed_w - svg_w * scale) / 2.0;
    let offset_y = (bed_h - svg_h * scale) / 2.0;

    let mut paint_groups = Vec::new();
    collect_paths_by_paint(
        tree.root(),
        scale,
        offset_x,
        offset_y,
        usvg::Transform::default(),
        None,
        &mut paint_groups,
    );
    let route_colors = route_colors
        && paint_groups.len() <= MAX_AUTO_COLOR_LAYERS
        && should_auto_route_import_colors(
            paint_groups
                .iter()
                .map(|group| group.display_color.as_deref())
                .chain(
                    text_elements
                        .iter()
                        .map(|text| Some(text.color_tag.as_str())),
                ),
        );

    // Create editable Text objects for extracted text elements.
    // `scale` maps SVG px -> mm (96 DPI).
    let px_to_mm = scale;
    for text_el in &text_elements {
        let font_size_mm = text_el.font_size_px * px_to_mm;
        let x = text_el.x * scale + offset_x;
        // SVG text y is baseline; convert to top-left using the font's actual
        // ascender (distance from baseline to top of tallest glyphs).
        let ascender = font_ascender_mm(
            &text_el.font_family,
            font_size_mm,
            text_el.bold,
            text_el.italic,
        );
        let y = text_el.y * scale + offset_y - ascender;

        // Estimate text width from content length (approximate — will be
        // resized to actual glyph metrics by the service layer after import).
        let est_width = font_size_mm * text_el.content.len() as f64 * 0.6;
        let bounds = Bounds::new(
            Point2D::new(x, y),
            Point2D::new(
                x + est_width.max(font_size_mm * 2.0),
                y + font_size_mm * 1.3,
            ),
        );

        let h_spacing_mm = text_el.h_spacing_px * px_to_mm;

        let destination = if route_colors {
            resolve_import_color_layer(project, &text_el.color_tag).unwrap_or(layer_id)
        } else {
            layer_id
        };
        let obj = ProjectObject::new(
            &format!("Text: {}", text_el.content),
            destination,
            bounds,
            ObjectData::Text {
                content: text_el.content.clone(),
                font_family: text_el.font_family.clone(),
                font_size_mm,
                alignment: text_el.alignment,
                alignment_v: text_el.alignment_v,
                bold: text_el.bold,
                italic: text_el.italic,
                upper_case: false,
                welded: false,
                h_spacing: h_spacing_mm,
                v_spacing: 0.0,
                on_path: false,
                path_offset: 0.0,
                distort: false,
                layout_mode: TextLayoutMode::Straight,
                rtl: text_el.rtl,
                bend_radius: 0.0,
                transform_style: crate::object::TextTransformStyle::None,
                transform_curve: 0.0,
                circle_placement: crate::object::TextCirclePlacement::TopOutside,
                max_width: None,
                squeeze: false,
                ignore_empty_vars: false,
                resolved_font_source: None,
                resolved_font_key: None,
                resolved_path_data: None,
                missing_font: false,
                missing_glyphs: Vec::new(),
                guide_path_id: None,
                variable_text: None,
            },
        );
        let id = obj.id;
        project.add_object(obj);
        created_ids.push(id);
    }

    // Preserve embedded raster artwork as real project image assets. usvg
    // already resolves data-URI PNG/JPEG/GIF/WebP payloads and computes the
    // complete ancestor/image transform, including preserveAspectRatio.
    // Keep the native pixels and express their SVG placement through Beam
    // Bench's bounds + affine transform model.
    import_embedded_svg_images(
        tree.root(),
        project,
        layer_id,
        scale,
        offset_x,
        offset_y,
        &mut created_ids,
    )?;

    // A file with this many distinct paints is color noise (flattened
    // gradient art), not operation intent. Import it as one object rather
    // than exploding the object list.
    if paint_groups.len() > MAX_AUTO_COLOR_LAYERS {
        let merged = paint_groups.into_iter().reduce(|mut acc, g| {
            if !acc.path_data.is_empty() && !g.path_data.is_empty() {
                acc.path_data.push(' ');
            }
            acc.path_data.push_str(&g.path_data);
            acc.has_closed |= g.has_closed;
            acc.min_x = acc.min_x.min(g.min_x);
            acc.min_y = acc.min_y.min(g.min_y);
            acc.max_x = acc.max_x.max(g.max_x);
            acc.max_y = acc.max_y.max(g.max_y);
            acc.display_color = None;
            acc
        });
        paint_groups = merged.into_iter().collect();
    }

    let multi_color = paint_groups.len() > 1;
    for group in paint_groups {
        if group.path_data.is_empty() {
            continue;
        }
        // Degenerate bbox guard: fall back to the root bounding box.
        let bounds = if group.min_x.is_finite() && group.max_x.is_finite() {
            Bounds::new(
                Point2D::new(group.min_x, group.min_y),
                Point2D::new(group.max_x, group.max_y),
            )
        } else {
            let root_bbox = tree.root().abs_bounding_box();
            Bounds::new(
                Point2D::new(
                    root_bbox.left() as f64 * scale + offset_x,
                    root_bbox.top() as f64 * scale + offset_y,
                ),
                Point2D::new(
                    root_bbox.right() as f64 * scale + offset_x,
                    root_bbox.bottom() as f64 * scale + offset_y,
                ),
            )
        };

        let name = match (&group.display_color, multi_color) {
            (Some(color), true) => format!("SVG Import {color}"),
            _ => "SVG Import".to_string(),
        };

        let destination = if route_colors {
            group
                .display_color
                .as_deref()
                .and_then(|color| resolve_import_color_layer(project, color))
                .unwrap_or(layer_id)
        } else {
            layer_id
        };
        let obj = ProjectObject::new(
            &name,
            destination,
            bounds,
            ObjectData::VectorPath {
                path_data: group.path_data.trim().to_string(),
                closed: group.has_closed,
                ruler_guide_axis: None,
            },
        );
        let id = obj.id;
        project.add_object(obj);
        created_ids.push(id);
    }

    Ok(created_ids)
}

/// Import a raster image (PNG/JPEG) into the project, storing it as an asset
/// and creating a RasterImage object on the specified layer.
/// Millimetres per pixel for a raster file, derived from its embedded DPI
/// metadata: PNG `pHYs`, JPEG JFIF density or EXIF resolution, TIFF
/// XResolution, or the BMP pixels-per-metre header. Defaults to 96 DPI when
/// the file carries none (GIF and TGA store no physical resolution; WebP
/// only carries it in an optional EXIF chunk, currently unread).
fn raster_mm_per_px(bytes: &[u8]) -> f64 {
    const DEFAULT_DPI: f64 = 96.0;
    let dpi = png_dpi(bytes)
        .or_else(|| jpeg_dpi(bytes))
        .or_else(|| tiff_dpi(bytes))
        .or_else(|| bmp_dpi(bytes))
        // Reject corrupt or absurd stored resolutions rather than producing
        // kilometre-scale or microscopic imports.
        .filter(|dpi| is_sane_dpi(*dpi))
        .unwrap_or(DEFAULT_DPI);
    25.4 / dpi
}

/// Stored resolutions outside this range are treated as corrupt metadata.
fn is_sane_dpi(dpi: f64) -> bool {
    dpi.is_finite() && (10.0..=10_000.0).contains(&dpi)
}

/// DPI from a PNG `pHYs` chunk (pixels per metre, unit flag 1).
fn png_dpi(bytes: &[u8]) -> Option<f64> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.len() < 8 || bytes[..8] != SIGNATURE {
        return None;
    }
    let mut pos = 8;
    while pos + 8 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
        let chunk_type = &bytes[pos + 4..pos + 8];
        if chunk_type == b"pHYs" {
            if len < 9 || pos + 8 + 9 > bytes.len() {
                return None;
            }
            let x_ppu = u32::from_be_bytes(bytes[pos + 8..pos + 12].try_into().ok()?);
            let unit_is_metre = bytes[pos + 16] == 1;
            if unit_is_metre && x_ppu > 0 {
                return Some(x_ppu as f64 * 0.0254);
            }
            return None;
        }
        if chunk_type == b"IDAT" || chunk_type == b"IEND" {
            return None;
        }
        pos += 12 + len; // length + type + data + crc
    }
    None
}

/// DPI from JPEG metadata: a JFIF APP0 density with real units (1 = dots per
/// inch, 2 = dots per cm) wins; otherwise the EXIF (APP1) resolution, which
/// is a TIFF structure parsed by `tiff_dpi`. Scanners typically write JFIF
/// density; cameras typically write EXIF.
fn jpeg_dpi(bytes: &[u8]) -> Option<f64> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut exif_dpi: Option<f64> = None;
    let mut pos = 2;
    while pos + 4 <= bytes.len() {
        if bytes[pos] != 0xFF {
            break;
        }
        let marker = bytes[pos + 1];
        if (0xD0..=0xD9).contains(&marker) {
            pos += 2;
            continue;
        }
        if marker == 0xDA {
            break; // start of scan: metadata segments are behind us
        }
        let seg_len = u16::from_be_bytes([bytes[pos + 2], bytes[pos + 3]]) as usize;
        if seg_len < 2 || pos + 2 + seg_len > bytes.len() {
            break;
        }
        let payload = &bytes[pos + 4..pos + 2 + seg_len];
        if marker == 0xE0 && payload.len() >= 12 && payload.starts_with(b"JFIF\0") {
            // identifier(5) version(2) units(1) xdensity(2) ydensity(2)
            let units = payload[7];
            let x_density = u16::from_be_bytes([payload[8], payload[9]]) as f64;
            let jfif = match units {
                1 if x_density > 0.0 => Some(x_density),
                2 if x_density > 0.0 => Some(x_density * 2.54),
                _ => None, // unit 0: aspect ratio only, keep looking
            };
            // Only a SANE JFIF density short-circuits the scan: a junk value
            // (e.g. 1 DPI from a sloppy encoder) must not shadow a valid
            // EXIF resolution later in the stream.
            if let Some(dpi) = jfif {
                if is_sane_dpi(dpi) {
                    return Some(dpi);
                }
            }
        } else if marker == 0xE1 && payload.len() > 6 && payload.starts_with(b"Exif\0\0") {
            // EXIF offsets are relative to the embedded TIFF header.
            exif_dpi = exif_dpi.or_else(|| tiff_dpi(&payload[6..]));
        }
        pos += 2 + seg_len;
    }
    exif_dpi
}

/// DPI from a TIFF IFD0: XResolution (tag 282, RATIONAL) interpreted via
/// ResolutionUnit (tag 296; 2 = inch, the TIFF default, 3 = cm, 1 = none).
/// Also parses the TIFF block embedded in JPEG EXIF segments.
fn tiff_dpi(bytes: &[u8]) -> Option<f64> {
    let little_endian = match bytes.get(0..4)? {
        [0x49, 0x49, 0x2A, 0x00] => true,
        [0x4D, 0x4D, 0x00, 0x2A] => false,
        _ => return None,
    };
    let read_u16 = |pos: usize| -> Option<u16> {
        let b: [u8; 2] = bytes.get(pos..pos + 2)?.try_into().ok()?;
        Some(if little_endian {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        })
    };
    let read_u32 = |pos: usize| -> Option<u32> {
        let b: [u8; 4] = bytes.get(pos..pos + 4)?.try_into().ok()?;
        Some(if little_endian {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        })
    };

    let ifd = read_u32(4)? as usize;
    let entry_count = read_u16(ifd)? as usize;
    let mut x_resolution: Option<f64> = None;
    let mut unit = 2u16; // TIFF default: inches
    for i in 0..entry_count.min(512) {
        let entry = ifd + 2 + i * 12;
        match read_u16(entry)? {
            282 => {
                // RATIONAL: the value field holds an offset to num/den.
                let offset = read_u32(entry + 8)? as usize;
                let numerator = read_u32(offset)? as f64;
                let denominator = read_u32(offset + 4)? as f64;
                if denominator > 0.0 {
                    x_resolution = Some(numerator / denominator);
                }
            }
            296 => unit = read_u16(entry + 8)?,
            _ => {}
        }
    }
    let resolution = x_resolution?;
    match unit {
        2 => Some(resolution),
        3 => Some(resolution * 2.54),
        _ => None, // 1 = no absolute unit
    }
}

/// DPI from a BMP info header's horizontal pixels-per-metre field.
fn bmp_dpi(bytes: &[u8]) -> Option<f64> {
    if bytes.len() < 46 || &bytes[0..2] != b"BM" {
        return None;
    }
    let info_header_size = u32::from_le_bytes(bytes[14..18].try_into().ok()?);
    if info_header_size < 40 {
        return None; // BITMAPCOREHEADER carries no resolution fields
    }
    let pixels_per_metre = i32::from_le_bytes(bytes[38..42].try_into().ok()?);
    (pixels_per_metre > 0).then(|| f64::from(pixels_per_metre) * 0.0254)
}

pub fn import_image(
    image_bytes: &[u8],
    filename: &str,
    source_path: Option<String>,
    project: &mut Project,
    layer_id: LayerId,
) -> Result<ObjectId, ImportError> {
    let img = beambench_raster::decode::decode_image_oriented(image_bytes)
        .map_err(|e| ImportError::ImageDecodeError(e.to_string()))?;

    let width = img.width();
    let height = img.height();

    // Convert to grayscale immediately on import — laser engravers only
    // use luminance data. Storing grayscale avoids color artifacts in
    // canvas preview and the Adjust Image dialog.
    let gray = image_to_engraving_grayscale(&img);
    let mut gray_png = Vec::new();
    {
        let encoder = image::codecs::png::PngEncoder::new(std::io::Cursor::new(&mut gray_png));
        image::ImageEncoder::write_image(
            encoder,
            gray.as_raw(),
            width,
            height,
            image::ExtendedColorType::L8,
        )
        .map_err(|e| ImportError::ImageDecodeError(format!("grayscale encode: {e}")))?;
    }

    let asset = Asset::new(
        filename,
        AssetMediaType::Png, // always PNG after grayscale conversion
        gray_png.len() as u64,
        Some(width),
        Some(height),
    )
    .with_source_path(source_path);
    let asset_id = asset.id;
    project.add_asset(asset, gray_png);

    // Physical size from the file's DPI metadata (PNG pHYs / JPEG JFIF),
    // defaulting to 96 DPI. Never scaled to fit the workspace: imports keep
    // their true measurements and the import pipeline warns when the result
    // is larger than the bed.
    let bed_w = project.workspace.bed_width_mm;
    let bed_h = project.workspace.bed_height_mm;
    let mm_per_px = raster_mm_per_px(image_bytes);
    let obj_w = width as f64 * mm_per_px;
    let obj_h = height as f64 * mm_per_px;
    // Center on the bed
    let x = (bed_w - obj_w) / 2.0;
    let y = (bed_h - obj_h) / 2.0;

    let bounds = Bounds::new(Point2D::new(x, y), Point2D::new(x + obj_w, y + obj_h));
    let obj = ProjectObject::new(
        filename,
        layer_id,
        bounds,
        ObjectData::RasterImage {
            asset_key: asset_id.to_string(),
            original_width_px: width,
            original_height_px: height,
            adjustments: None,
            masks: Vec::new(),
        },
    );
    let obj_id = obj.id;
    project.add_object(obj);
    Ok(obj_id)
}

/// Convert color artwork to laser luminance while treating transparent pixels
/// as white (no engraving). Shared with the engraving decoder so every path
/// treats transparency the same way.
fn image_to_engraving_grayscale(img: &image::DynamicImage) -> image::GrayImage {
    beambench_raster::decode::to_engraving_gray(img)
}

/// Recursively collect all path segments from a usvg group, transforming
/// each point to bed-space coordinates (SVG document → scaled + centered).
/// Paths accumulated per distinct SVG paint (stroke + fill), in first-seen
/// document order. Laser designs conventionally encode operation intent as
/// color (red = cut, black = engrave, ...), so each distinct paint imports as
/// its own selectable object instead of everything flattening into one.
struct PaintGroup {
    /// Identity key: stroke and fill paints, e.g. `s:#FF0000|f:none`.
    key: String,
    /// Solid color for layer routing and names (stroke preferred, else fill).
    display_color: Option<String>,
    path_data: String,
    has_closed: bool,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

fn paint_hex(paint: Option<&usvg::Paint>) -> Option<String> {
    match paint? {
        usvg::Paint::Color(c) => Some(format!("#{:02X}{:02X}{:02X}", c.red, c.green, c.blue)),
        // Gradients and patterns have no single color; group them together
        // under a non-hex marker rather than splitting per gradient.
        _ => Some("other".to_string()),
    }
}

fn collect_paths_by_paint(
    group: &usvg::Group,
    scale: f64,
    offset_x: f64,
    offset_y: f64,
    text_transform: usvg::Transform,
    clip: Option<&VecPath>,
    groups: &mut Vec<PaintGroup>,
) {
    for node in group.children() {
        match node {
            usvg::Node::Path(path) => {
                // Hidden shapes are not artwork, and must never burn.
                if !path.is_visible() {
                    continue;
                }
                let mut bed_path = transform_path_to_bed_space(
                    path,
                    &text_transform.pre_concat(path.abs_transform()),
                    scale,
                    offset_x,
                    offset_y,
                );
                if let Some(clip) = clip {
                    if let Some(fill) = path.fill() {
                        bed_path = normalize_svg_fill(bed_path, fill.rule());
                    }
                    bed_path = clip_to_region(&bed_path, path.fill().is_some(), clip);
                }
                cleanup_vecpath_for_planner(&mut bed_path, VECTOR_PATH_FLATTEN_TOLERANCE_MM);
                if flatten_vecpath(&bed_path, VECTOR_PATH_FLATTEN_TOLERANCE_MM).is_empty() {
                    continue;
                }
                let Some(bbox) = bed_path.visual_bounds() else {
                    continue;
                };
                let d = bed_path.to_svg_d();
                let stroke_hex = paint_hex(path.stroke().map(|s| s.paint()));
                let fill_hex = paint_hex(path.fill().map(|f| f.paint()));
                let key = format!(
                    "s:{}|f:{}",
                    stroke_hex.as_deref().unwrap_or("none"),
                    fill_hex.as_deref().unwrap_or("none")
                );

                let entry = match groups.iter_mut().find(|g| g.key == key) {
                    Some(existing) => existing,
                    None => {
                        let display_color = [stroke_hex.as_deref(), fill_hex.as_deref()]
                            .into_iter()
                            .flatten()
                            .find(|hex| hex.starts_with('#'))
                            .map(str::to_string);
                        groups.push(PaintGroup {
                            key,
                            display_color,
                            path_data: String::new(),
                            has_closed: false,
                            min_x: f64::INFINITY,
                            min_y: f64::INFINITY,
                            max_x: f64::NEG_INFINITY,
                            max_y: f64::NEG_INFINITY,
                        });
                        groups.last_mut().expect("group was just pushed")
                    }
                };

                if bed_path.subpaths.iter().any(|subpath| subpath.closed) {
                    entry.has_closed = true;
                }
                if !entry.path_data.is_empty() {
                    entry.path_data.push(' ');
                }
                entry.path_data.push_str(&d);
                entry.min_x = entry.min_x.min(bbox.min.x);
                entry.min_y = entry.min_y.min(bbox.min.y);
                entry.max_x = entry.max_x.max(bbox.max.x);
                entry.max_y = entry.max_y.max(bbox.max.y);
            }
            usvg::Node::Group(g) => {
                let own_clip = g.clip_path().map(|clip_path| {
                    clip_region(g, clip_path, scale, offset_x, offset_y, text_transform)
                });
                let combined = match (clip, own_clip) {
                    (Some(outer), Some(inner)) => Some(path_intersection_with_tolerance(
                        outer,
                        &inner,
                        VECTOR_PATH_FLATTEN_TOLERANCE_MM,
                    )),
                    (outer, inner) => inner.or_else(|| outer.cloned()),
                };
                collect_paths_by_paint(
                    g,
                    scale,
                    offset_x,
                    offset_y,
                    text_transform,
                    combined.as_ref(),
                    groups,
                );
            }
            // Text that was not extracted as editable is drawn as outlines.
            usvg::Node::Text(text) => {
                // Flattened glyph paths are local to the text. Their parent
                // transform still positions rotated labels and <use> copies.
                collect_paths_by_paint(
                    text.flattened(),
                    scale,
                    offset_x,
                    offset_y,
                    text_transform.pre_concat(text.abs_transform()),
                    clip,
                    groups,
                );
            }
            usvg::Node::Image(_) => {} // Imported by their dedicated pass.
        }
    }
}

/// The region a `clip-path` keeps, in bed space. Clip contents are positioned
/// by the clipped group's transform, then the clip path's own transform.
fn clip_region(
    group: &usvg::Group,
    clip_path: &usvg::ClipPath,
    scale: f64,
    offset_x: f64,
    offset_y: f64,
    text_transform: usvg::Transform,
) -> VecPath {
    fn collect(
        clip_group: &usvg::Group,
        base: &usvg::Transform,
        scale: f64,
        offset_x: f64,
        offset_y: f64,
        region: &mut Vec<VecPath>,
    ) {
        for node in clip_group.children() {
            match node {
                usvg::Node::Path(path) if path.is_visible() => {
                    let t = base.pre_concat(path.abs_transform());
                    let bed_path = transform_path_to_bed_space(path, &t, scale, offset_x, offset_y);
                    region.push(normalize_svg_fill(
                        bed_path,
                        path.fill().map(|fill| fill.rule()).unwrap_or_default(),
                    ));
                }
                usvg::Node::Group(child) => collect(child, base, scale, offset_x, offset_y, region),
                usvg::Node::Text(text) => collect(
                    text.flattened(),
                    &base.pre_concat(text.abs_transform()),
                    scale,
                    offset_x,
                    offset_y,
                    region,
                ),
                _ => {}
            }
        }
    }
    let base = text_transform
        .pre_concat(group.abs_transform())
        .pre_concat(clip_path.transform());
    let mut parts = Vec::new();
    collect(
        clip_path.root(),
        &base,
        scale,
        offset_x,
        offset_y,
        &mut parts,
    );
    let mut region = weld_shapes_with_tolerance(&parts, VECTOR_PATH_FLATTEN_TOLERANCE_MM);
    if let Some(inner) = clip_path.clip_path() {
        region = path_intersection_with_tolerance(
            &region,
            &clip_region(group, inner, scale, offset_x, offset_y, text_transform),
            VECTOR_PATH_FLATTEN_TOLERANCE_MM,
        );
    }
    region
}

fn normalize_svg_fill(mut path: VecPath, rule: usvg::FillRule) -> VecPath {
    // SVG fills implicitly close open contours.
    for subpath in &mut path.subpaths {
        if !subpath.closed {
            subpath.commands.push(PathCommand::Close);
            subpath.closed = true;
        }
    }
    normalize_subject_with_tolerance(
        &[path],
        VECTOR_PATH_FLATTEN_TOLERANCE_MM,
        VECTOR_PATH_FLATTEN_TOLERANCE_MM,
        rule == usvg::FillRule::NonZero,
    )
}

/// Keep the part of `path` inside `region`. Filled shapes become the
/// overlapping area; stroke-only lines are cut where they leave the region,
/// as a viewer draws them.
fn clip_to_region(path: &VecPath, filled: bool, region: &VecPath) -> VecPath {
    if filled {
        return path_intersection_with_tolerance(path, region, VECTOR_PATH_FLATTEN_TOLERANCE_MM);
    }
    let rings: Vec<Vec<Point2D>> = flatten_vecpath(region, VECTOR_PATH_FLATTEN_TOLERANCE_MM)
        .into_iter()
        .filter(|ring| ring.points.len() >= 3)
        .map(|ring| ring.points)
        .collect();
    let inside = |p: Point2D| {
        let mut crossings = 0;
        for ring in &rings {
            for (i, a) in ring.iter().enumerate() {
                let b = ring[(i + 1) % ring.len()];
                if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y)
                {
                    crossings += 1;
                }
            }
        }
        crossings % 2 == 1
    };
    let mut subpaths = Vec::new();
    for line in flatten_vecpath(path, VECTOR_PATH_FLATTEN_TOLERANCE_MM) {
        let mut points = line.points;
        if line.closed && points.len() > 1 {
            points.push(points[0]);
        }
        let mut current: Vec<Point2D> = Vec::new();
        let mut flush = |current: &mut Vec<Point2D>| {
            if current.len() >= 2 {
                let mut subpath = SubPath::new();
                subpath.commands.push(PathCommand::MoveTo {
                    x: current[0].x,
                    y: current[0].y,
                });
                for p in &current[1..] {
                    subpath
                        .commands
                        .push(PathCommand::LineTo { x: p.x, y: p.y });
                }
                subpaths.push(subpath);
            }
            current.clear();
        };
        for pair in points.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            // Split the segment wherever it crosses the region's boundary.
            let mut cuts = vec![0.0, 1.0];
            for ring in &rings {
                for (i, a) in ring.iter().enumerate() {
                    let b = ring[(i + 1) % ring.len()];
                    if let Some(t) = segment_crossing(start, end, *a, b) {
                        cuts.push(t);
                    }
                }
            }
            cuts.sort_by(f64::total_cmp);
            cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
            let at = |t: f64| {
                Point2D::new(
                    start.x + (end.x - start.x) * t,
                    start.y + (end.y - start.y) * t,
                )
            };
            for span in cuts.windows(2) {
                if inside(at((span[0] + span[1]) / 2.0)) {
                    if current.is_empty() {
                        current.push(at(span[0]));
                    }
                    current.push(at(span[1]));
                } else {
                    flush(&mut current);
                }
            }
        }
        flush(&mut current);
    }
    VecPath { subpaths }
}

/// Parameter along `p0 -> p1` where it crosses `q0 -> q1`, if it does.
fn segment_crossing(p0: Point2D, p1: Point2D, q0: Point2D, q1: Point2D) -> Option<f64> {
    let r = (p1.x - p0.x, p1.y - p0.y);
    let s = (q1.x - q0.x, q1.y - q0.y);
    let denominator = r.0 * s.1 - r.1 * s.0;
    if denominator.abs() < 1e-15 {
        return None;
    }
    let qp = (q0.x - p0.x, q0.y - p0.y);
    let t = (qp.0 * s.1 - qp.1 * s.0) / denominator;
    let u = (qp.0 * r.1 - qp.1 * r.0) / denominator;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

fn import_embedded_svg_images(
    group: &usvg::Group,
    project: &mut Project,
    layer_id: LayerId,
    scale: f64,
    offset_x: f64,
    offset_y: f64,
    created_ids: &mut Vec<ObjectId>,
) -> Result<(), ImportError> {
    for node in group.children() {
        match node {
            usvg::Node::Group(child) => import_embedded_svg_images(
                child,
                project,
                layer_id,
                scale,
                offset_x,
                offset_y,
                created_ids,
            )?,
            usvg::Node::Image(image) if image.is_visible() => {
                let (bytes, extension) = match image.kind() {
                    usvg::ImageKind::JPEG(bytes) => (bytes.as_slice(), "jpg"),
                    usvg::ImageKind::PNG(bytes) => (bytes.as_slice(), "png"),
                    usvg::ImageKind::GIF(bytes) => (bytes.as_slice(), "gif"),
                    usvg::ImageKind::WEBP(bytes) => (bytes.as_slice(), "webp"),
                    // A nested SVG image needs a separate vector-coordinate
                    // import path. Do not silently rasterize it and discard
                    // its vector fidelity or operation colors.
                    usvg::ImageKind::SVG(_) => continue,
                };
                let filename = format!("SVG Embedded Image {}.{extension}", created_ids.len() + 1);
                let object_id = import_image(bytes, &filename, None, project, layer_id)?;

                let native_width = image.size().width() as f64;
                let native_height = image.size().height() as f64;
                let bounds = Bounds::new(
                    Point2D::new(offset_x, offset_y),
                    Point2D::new(
                        offset_x + native_width * scale,
                        offset_y + native_height * scale,
                    ),
                );
                let base_center = Point2D::new(
                    offset_x + native_width * scale / 2.0,
                    offset_y + native_height * scale / 2.0,
                );
                let svg_transform = image.abs_transform();
                let (placed_center_x, placed_center_y) = to_bed_space(
                    native_width / 2.0,
                    native_height / 2.0,
                    &svg_transform,
                    scale,
                    offset_x,
                    offset_y,
                );
                let transform = Transform2D {
                    a: svg_transform.sx as f64,
                    b: svg_transform.ky as f64,
                    c: svg_transform.kx as f64,
                    d: svg_transform.sy as f64,
                    tx: placed_center_x - base_center.x,
                    ty: placed_center_y - base_center.y,
                };

                let object = project.find_object_mut(object_id).ok_or_else(|| {
                    ImportError::ImageDecodeError(
                        "embedded SVG image object was not created".to_string(),
                    )
                })?;
                object.bounds = bounds;
                object.transform = transform;
                created_ids.push(object_id);
            }
            _ => {}
        }
    }
    Ok(())
}

/// Convert a single usvg path's segments to an SVG d-string with all
/// coordinates transformed from path-local space → SVG document space → bed space.
fn transform_path_to_bed_space(
    path: &usvg::Path,
    t: &usvg::Transform,
    scale: f64,
    offset_x: f64,
    offset_y: f64,
) -> VecPath {
    let t = *t;
    let mut subpaths = Vec::new();
    let mut current = SubPath::new();

    for seg in path.data().segments() {
        match seg {
            PathSegment::MoveTo(pt) => {
                if !current.commands.is_empty() {
                    subpaths.push(current);
                    current = SubPath::new();
                }
                let (x, y) = to_bed_space(pt.x as f64, pt.y as f64, &t, scale, offset_x, offset_y);
                current.commands.push(PathCommand::MoveTo { x, y });
            }
            PathSegment::LineTo(pt) => {
                let (x, y) = to_bed_space(pt.x as f64, pt.y as f64, &t, scale, offset_x, offset_y);
                current.commands.push(PathCommand::LineTo { x, y });
            }
            PathSegment::QuadTo(p1, p2) => {
                let (cx, cy) =
                    to_bed_space(p1.x as f64, p1.y as f64, &t, scale, offset_x, offset_y);
                let (x, y) = to_bed_space(p2.x as f64, p2.y as f64, &t, scale, offset_x, offset_y);
                current.commands.push(PathCommand::QuadTo { cx, cy, x, y });
            }
            PathSegment::CubicTo(p1, p2, p3) => {
                let (c1x, c1y) =
                    to_bed_space(p1.x as f64, p1.y as f64, &t, scale, offset_x, offset_y);
                let (c2x, c2y) =
                    to_bed_space(p2.x as f64, p2.y as f64, &t, scale, offset_x, offset_y);
                let (x, y) = to_bed_space(p3.x as f64, p3.y as f64, &t, scale, offset_x, offset_y);
                current.commands.push(PathCommand::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                });
            }
            PathSegment::Close => {
                current.commands.push(PathCommand::Close);
                current.closed = true;
            }
        }
    }

    if !current.commands.is_empty() {
        subpaths.push(current);
    }

    VecPath { subpaths }
}

/// Transform a point from path-local space to bed space:
/// 1. Apply the usvg absolute transform (local → SVG document space)
/// 2. Scale and offset to fit bed (document → bed space)
fn to_bed_space(
    x: f64,
    y: f64,
    t: &usvg::Transform,
    scale: f64,
    offset_x: f64,
    offset_y: f64,
) -> (f64, f64) {
    let doc_x = t.sx as f64 * x + t.kx as f64 * y + t.tx as f64;
    let doc_y = t.ky as f64 * x + t.sy as f64 * y + t.ty as f64;
    (doc_x * scale + offset_x, doc_y * scale + offset_y)
}

/// Convert usvg path segments into a VecPath, then serialize to SVG d-string.
#[cfg(test)]
fn usvg_segments_to_svg_d(segments: impl Iterator<Item = PathSegment>) -> String {
    let path = usvg_segments_to_vecpath(segments);
    path.to_svg_d()
}

/// Convert usvg path segments to a structured VecPath.
pub fn usvg_segments_to_vecpath(segments: impl Iterator<Item = PathSegment>) -> VecPath {
    let mut subpaths = Vec::new();
    let mut current = SubPath::new();

    for seg in segments {
        match seg {
            PathSegment::MoveTo(pt) => {
                if !current.commands.is_empty() {
                    subpaths.push(current);
                    current = SubPath::new();
                }
                current.commands.push(PathCommand::MoveTo {
                    x: pt.x as f64,
                    y: pt.y as f64,
                });
            }
            PathSegment::LineTo(pt) => {
                current.commands.push(PathCommand::LineTo {
                    x: pt.x as f64,
                    y: pt.y as f64,
                });
            }
            PathSegment::QuadTo(p1, p2) => {
                current.commands.push(PathCommand::QuadTo {
                    cx: p1.x as f64,
                    cy: p1.y as f64,
                    x: p2.x as f64,
                    y: p2.y as f64,
                });
            }
            PathSegment::CubicTo(p1, p2, p3) => {
                current.commands.push(PathCommand::CubicTo {
                    c1x: p1.x as f64,
                    c1y: p1.y as f64,
                    c2x: p2.x as f64,
                    c2y: p2.y as f64,
                    x: p3.x as f64,
                    y: p3.y as f64,
                });
            }
            PathSegment::Close => {
                current.commands.push(PathCommand::Close);
                current.closed = true;
            }
        }
    }

    if !current.commands.is_empty() {
        subpaths.push(current);
    }

    VecPath { subpaths }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_project() -> Project {
        Project::new("Import Test")
    }

    fn first_layer_id(project: &mut Project) -> LayerId {
        project.ensure_default_layer()
    }

    #[test]
    fn import_color_matching_handles_alpha_and_rejects_malformed_tags() {
        assert_eq!(
            normalize_import_color_tag(" #aB12ef80 ").as_deref(),
            Some("#AB12EF")
        );
        for invalid in ["#12345éx", "#123456zz", "#123", "red", "#éabcd"] {
            assert_eq!(normalize_import_color_tag(invalid), None, "{invalid}");
        }
        let mut project = test_project();
        for operation in [OperationType::Image, OperationType::Tool] {
            let mut layer = Layer::new("Not a vector operation", operation);
            layer.color_tag = ColorTag("#123456".into());
            project.add_layer(layer);
        }
        let id = resolve_import_color_layer(&mut project, "#123456").unwrap();
        let imported = project.find_layer(id).unwrap().clone();
        assert!(!imported.is_tool_layer);
        assert_eq!(imported.primary_entry().operation, OperationType::Line);
        assert_eq!(
            resolve_import_color_layer(&mut project, "#123456FF"),
            Some(id)
        );
        assert_eq!(project.find_layer(id), Some(&imported));
    }

    #[test]
    fn svg_color_layers_preserve_geometry_and_share_colors_across_paint_groups() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="60mm" viewBox="0 0 100 60">
            <g transform="translate(5 3)">
                <rect x="0" y="0" width="10" height="20" fill="red"/>
                <path d="M20 0 L30 20" fill="none" stroke="rgb(255,0,0)"/>
                <rect x="40" y="0" width="10" height="20" fill="blue"/>
                <rect x="60" y="0" width="10" height="20" fill="blue" stroke="red"/>
            </g>
        </svg>"##;
        let mut original = test_project();
        let requested = first_layer_id(&mut original);
        let mut routed = original.clone();
        let expected = import_svg(svg, &mut original, requested).unwrap();
        let ids = import_svg_with_color_layers(svg, &mut routed, requested).unwrap();
        assert_eq!(ids.len(), 4);
        for (id, expected) in ids.iter().zip(expected) {
            let object = routed.find_object(*id).unwrap();
            let before = original.find_object(expected).unwrap();
            assert_eq!(object.bounds, before.bounds);
            assert_eq!(object.data, before.data);
            assert_eq!(object.transform, before.transform);
        }
        let layers: Vec<_> = ids
            .iter()
            .map(|id| routed.find_object(*id).unwrap().layer_id)
            .collect();
        assert_eq!(layers[0], layers[1]);
        assert_eq!(
            layers[0], layers[3],
            "outline color takes precedence, as for PDF/AI"
        );
        assert_ne!(layers[0], layers[2]);
        assert_eq!(routed.layers.len(), 3);
    }

    #[test]
    fn svg_editable_text_keeps_css_and_inherited_colors() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <style>.blue { fill: blue; }</style>
            <g color="#123456" fill="currentColor">
                <text x="10" y="40" font-family="sans-serif">Inherited</text>
            </g>
            <text id="blue-label" class="blue" x="10" y="80" font-family="sans-serif">CSS</text>
            <text/>
            <text x="10" y="120" fill="red" style="fill:blue" font-family="sans-serif">Style</text>
        </svg>"##;
        let mut project = test_project();
        let requested = first_layer_id(&mut project);
        let ids = import_svg_with_color_layers(svg, &mut project, requested).unwrap();
        assert_eq!(ids.len(), 3);
        for (id, color) in ids.iter().zip(["#123456", "#0000FF", "#0000FF"]) {
            let object = project.find_object(*id).unwrap();
            assert!(matches!(object.data, ObjectData::Text { .. }));
            assert_eq!(
                project.find_layer(object.layer_id).unwrap().color_tag.0,
                color
            );
        }
        assert_eq!(project.layers.len(), 3);
    }

    #[test]
    fn svg_multicolor_text_uses_outlines_and_hidden_labels_do_not_import() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <style>.hidden { display:none; }</style>
            <text x="10" y="50" font-family="sans-serif"><tspan fill="red">Red</tspan><tspan fill="blue">Blue</tspan></text>
            <text class="hidden" x="10" y="80" fill="green" font-family="sans-serif">Hidden</text>
        </svg>"##;
        let mut project = test_project();
        let requested = first_layer_id(&mut project);
        let ids = import_svg_with_color_layers(svg, &mut project, requested).unwrap();
        assert_eq!(ids.len(), 2);
        for (id, color) in ids.iter().zip(["#FF0000", "#0000FF"]) {
            let object = project.find_object(*id).unwrap();
            assert!(matches!(object.data, ObjectData::VectorPath { .. }));
            assert_eq!(
                project.find_layer(object.layer_id).unwrap().color_tag.0,
                color
            );
        }
        assert_eq!(project.layers.len(), 3);
    }

    #[test]
    fn svg_reused_text_keeps_every_instance() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
            <text id="label" x="10" y="50" fill="red" font-family="sans-serif">Label</text>
            <use href="#label" x="100"/>
        </svg>"##;
        let text = std::str::from_utf8(svg).unwrap();
        let group_reference = text
            .replace("<text id=\"label\"", "<g id=\"label\"><text")
            .replace("</text>", "</text></g>");
        for svg in [text, &group_reference] {
            let mut project = test_project();
            let requested = first_layer_id(&mut project);
            let ids =
                import_svg_with_color_layers(svg.as_bytes(), &mut project, requested).unwrap();
            assert_eq!(ids.len(), 1);
            let object = project.find_object(ids[0]).unwrap();
            assert!(matches!(object.data, ObjectData::VectorPath { .. }));
            assert!(
                object.bounds.width() > 100.0 * 25.4 / 96.0,
                "both copies must be present"
            );
            assert_eq!(
                project.find_layer(object.layer_id).unwrap().color_tag.0,
                "#FF0000"
            );
        }
    }

    #[test]
    fn svg_text_attribute_selector_css_keeps_original_colors() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
            <style>text { fill:blue } text[id] { fill:red }</style>
            <text x="0" y="20" font-family="sans-serif">Anonymous</text>
            <text id="named" x="0" y="50" font-family="sans-serif">Named</text>
        </svg>"##;
        let mut project = test_project();
        let requested = first_layer_id(&mut project);
        let ids = import_svg_with_color_layers(svg, &mut project, requested).unwrap();
        let colors: Vec<_> = ids
            .iter()
            .map(|id| {
                let object = project.find_object(*id).unwrap();
                project
                    .find_layer(object.layer_id)
                    .unwrap()
                    .color_tag
                    .0
                    .as_str()
            })
            .collect();
        assert_eq!(colors, ["#0000FF", "#FF0000"]);
    }

    #[test]
    fn svg_outlined_text_keeps_parent_transform_and_clip() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">
            <defs><clipPath id="clip"><rect width="20" height="30"/></clipPath></defs>
            <g transform="translate(50 20)" clip-path="url(#clip)">
                <text x="0" y="20" fill="red" font-family="sans-serif" font-size="20">MMMM</text>
            </g>
        </svg>"##;
        let mut project = test_project();
        let requested = first_layer_id(&mut project);
        let ids = import_svg_with_color_layers(svg, &mut project, requested).unwrap();
        assert_eq!(ids.len(), 1);
        let object = project.find_object(ids[0]).unwrap();
        assert!(matches!(object.data, ObjectData::VectorPath { .. }));
        // A 100 mm SVG is centered at x=150 on the default 400 mm bed.
        assert!(object.bounds.min.x >= 200.0 - 1e-5, "{:?}", object.bounds);
        assert!(object.bounds.max.x <= 220.0 + 1e-5, "{:?}", object.bounds);
        assert!(object.bounds.width() > 1.0);
    }

    #[test]
    fn svg_color_cap_includes_editable_labels() {
        let labels = (0..=MAX_AUTO_COLOR_LAYERS)
            .map(|i| {
                format!(
                    r##"<text x="0" y="{}" fill="#{:06X}" font-family="sans-serif">Label</text>"##,
                    20 * (i + 1),
                    i + 1
                )
            })
            .collect::<String>();
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="400">{labels}</svg>"#
        );
        let mut project = test_project();
        let requested = first_layer_id(&mut project);
        let ids = import_svg_with_color_layers(svg.as_bytes(), &mut project, requested).unwrap();
        assert_eq!(ids.len(), MAX_AUTO_COLOR_LAYERS + 1);
        assert!(
            ids.iter()
                .all(|id| project.find_object(*id).unwrap().layer_id == requested)
        );
        assert_eq!(project.layers.len(), 1);
    }

    #[test]
    fn svg_gradients_and_color_noise_keep_requested_layer() {
        let gradient = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <defs><linearGradient id="g"><stop stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient></defs>
            <rect width="10" height="10" fill="url(#g)"/>
        </svg>"##;
        let body = (0..=MAX_AUTO_COLOR_LAYERS)
            .map(|i| {
                format!(
                    r##"<rect x="{}" width="5" height="10" fill="#{:06X}"/>"##,
                    i * 10,
                    i + 1
                )
            })
            .collect::<String>();
        let noise = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">{body}</svg>"#
        );
        for svg in [gradient, &noise] {
            let mut project = test_project();
            let requested = first_layer_id(&mut project);
            let ids =
                import_svg_with_color_layers(svg.as_bytes(), &mut project, requested).unwrap();
            assert_eq!(ids.len(), 1);
            assert_eq!(project.find_object(ids[0]).unwrap().layer_id, requested);
            assert_eq!(project.layers.len(), 1);
        }
    }

    #[test]
    fn parse_svg_font_size_units() {
        // px and bare numbers pass through
        assert_eq!(parse_svg_font_size("24px"), 24.0);
        assert_eq!(parse_svg_font_size("24"), 24.0);
        assert_eq!(parse_svg_font_size("13.5"), 13.5);
        // pt: 1pt = 96/72 px
        assert!((parse_svg_font_size("12pt") - 16.0).abs() < 1e-9);
        // pc: 1pc = 16px
        assert!((parse_svg_font_size("2pc") - 32.0).abs() < 1e-9);
        // mm: 1mm = 96/25.4 px
        assert!((parse_svg_font_size("25.4mm") - 96.0).abs() < 1e-9);
        // cm: 1cm = 96/2.54 px
        assert!((parse_svg_font_size("2.54cm") - 96.0).abs() < 1e-9);
        // in: 1in = 96px
        assert!((parse_svg_font_size("1in") - 96.0).abs() < 1e-9);
        // em / rem: relative to 16px root
        assert!((parse_svg_font_size("1.5em") - 24.0).abs() < 1e-9);
        assert!((parse_svg_font_size("2rem") - 32.0).abs() < 1e-9);
        // %: of 16px
        assert!((parse_svg_font_size("150%") - 24.0).abs() < 1e-9);
        // whitespace and case tolerated
        assert!((parse_svg_font_size(" 12PT ") - 16.0).abs() < 1e-9);
        // unknown units and garbage fall back to 16px
        assert_eq!(parse_svg_font_size("3vw"), 16.0);
        assert_eq!(parse_svg_font_size("large"), 16.0);
        assert_eq!(parse_svg_font_size(""), 16.0);
    }

    #[test]
    fn import_simple_svg_creates_path_objects() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <rect x="10" y="10" width="80" height="80"/>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        assert!(!ids.is_empty());

        let obj = project.find_object(ids[0]).unwrap();
        match &obj.data {
            ObjectData::VectorPath {
                path_data, closed, ..
            } => {
                assert!(!path_data.is_empty());
                assert!(*closed, "rect should be a closed path");
            }
            _ => panic!("Expected VectorPath, got {:?}", obj.data),
        }
    }

    #[test]
    fn import_svg_same_paint_paths_create_single_object() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <rect x="10" y="10" width="50" height="50" fill="black"/>
            <circle cx="120" cy="120" r="25" fill="black"/>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        assert_eq!(
            ids.len(),
            1,
            "Same-color SVG elements should stay one VectorPath"
        );

        let obj = project.find_object(ids[0]).unwrap();
        assert_eq!(obj.name, "SVG Import");
        match &obj.data {
            ObjectData::VectorPath { path_data, .. } => {
                let m_count = path_data.matches('M').count();
                assert!(
                    m_count >= 2,
                    "Expected at least 2 sub-paths (M commands), got {m_count}"
                );
            }
            _ => panic!("Expected VectorPath, got {:?}", obj.data),
        }
    }

    #[test]
    fn import_svg_distinct_colors_create_separate_objects() {
        // Color encodes operation intent (cut vs engrave); each distinct
        // paint must arrive as its own selectable object — report #13.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <rect x="10" y="10" width="50" height="50" fill="#FF0000"/>
            <rect x="100" y="10" width="50" height="50" fill="#FF0000"/>
            <circle cx="50" cy="150" r="25" fill="none" stroke="#0000FF"/>
        </svg>"##;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        assert_eq!(ids.len(), 2, "two distinct paints -> two objects");

        let red = project.find_object(ids[0]).unwrap();
        assert_eq!(red.name, "SVG Import #FF0000");
        match &red.data {
            ObjectData::VectorPath {
                path_data, closed, ..
            } => {
                assert!(*closed, "filled rects are closed");
                let m_count = path_data.matches('M').count();
                assert!(m_count >= 2, "both red rects share one object");
            }
            other => panic!("Expected VectorPath, got {other:?}"),
        }

        let blue = project.find_object(ids[1]).unwrap();
        assert_eq!(blue.name, "SVG Import #0000FF");

        // Per-object bounds, not the shared design bounds: the red pair sits
        // in the upper half, the blue circle in the lower-left quarter.
        assert!(
            red.bounds.max.y < blue.bounds.min.y,
            "red bounds {:?} should end above blue bounds {:?}",
            red.bounds,
            blue.bounds
        );
        assert!(blue.bounds.width() < 60.0, "circle bounds stay tight");
    }

    #[test]
    fn import_svg_discards_non_drawable_color_artifacts() {
        // Regression for the Facebook report: the green paint group contains
        // only zero-length geometry. Older releases imported it as
        // `SVG Import #00FF00`, then the planner failed once per cut pass.
        // The same-paint black artifact also must not inflate the real
        // rectangle's object bounds.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <rect x="10" y="10" width="50" height="50" fill="none" stroke="#000000"/>
            <path d="M190 190 L190 190" fill="none" stroke="#000000"/>
            <path d="M70 70 L70 70" fill="none" stroke="#00FF00"/>
            <path d="M80 80 Z" fill="none" stroke="#00FF00"/>
        </svg>"##;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        assert_eq!(ids.len(), 1, "empty green paint group must be discarded");

        let object = project.find_object(ids[0]).unwrap();
        assert_eq!(object.name, "SVG Import");
        assert!(
            crate::vector::normalize_object(object).is_some(),
            "every imported vector object must be planner-normalizable"
        );
        let expected_width_mm = 50.0 * 25.4 / 96.0;
        assert!(
            (object.bounds.width() - expected_width_mm).abs() < 0.01,
            "degenerate same-paint geometry must not inflate bounds: {:?}",
            object.bounds
        );
    }

    #[test]
    fn import_svg_color_noise_falls_back_to_single_object() {
        // More distinct paints than any operation palette: flattened
        // gradient art. Must not explode into per-color objects.
        let mut body = String::new();
        for i in 0..20 {
            body.push_str(&format!(
                r##"<rect x="{}" y="10" width="8" height="8" fill="#{:02X}0000"/>"##,
                10 + i * 9,
                10 + i * 12,
            ));
        }
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">{body}</svg>"#
        );
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let ids = import_svg(svg.as_bytes(), &mut project, layer_id).unwrap();
        assert_eq!(ids.len(), 1, "color-noise SVG imports as one object");
        let obj = project.find_object(ids[0]).unwrap();
        assert_eq!(obj.name, "SVG Import");
        match &obj.data {
            ObjectData::VectorPath { path_data, .. } => {
                assert_eq!(path_data.matches('M').count(), 20, "all paths kept");
            }
            other => panic!("Expected VectorPath, got {other:?}"),
        }
    }

    #[test]
    fn import_svg_keeps_true_size_when_larger_than_bed() {
        // A 2000x2000px SVG is 529.17mm square — larger than the 400x400mm
        // bed. Dimensions must be preserved exactly (never scaled to fit);
        // the import pipeline warns instead and preflight guards the cut.
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2000" height="2000">
            <rect x="0" y="0" width="2000" height="2000"/>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        assert_eq!(ids.len(), 1);

        let obj = project.find_object(ids[0]).unwrap();
        let expected_mm = 2000.0 * 25.4 / 96.0; // 529.166…
        assert!(
            (obj.bounds.width() - expected_mm).abs() < 0.1,
            "true width must be preserved: got {} expected {expected_mm}",
            obj.bounds.width()
        );
        assert!(
            (obj.bounds.height() - expected_mm).abs() < 0.1,
            "true height must be preserved: got {} expected {expected_mm}",
            obj.bounds.height()
        );
        // Centered on the bed, overhanging symmetrically.
        let bed_w = project.workspace.bed_width_mm;
        let center_x = (obj.bounds.min.x + obj.bounds.max.x) / 2.0;
        assert!(
            (center_x - bed_w / 2.0).abs() < 1.0,
            "oversized import should center on the bed"
        );
    }

    #[test]
    fn import_svg_centers_within_bed() {
        // A 500px (132.3mm) SVG fits the 400x400 bed at true size.
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="500" height="500">
            <rect x="0" y="0" width="500" height="500"/>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        assert_eq!(ids.len(), 1);

        let obj = project.find_object(ids[0]).unwrap();
        let bed_w = project.workspace.bed_width_mm;
        let bed_h = project.workspace.bed_height_mm;

        assert!(
            obj.bounds.min.x >= -0.1,
            "Object should not extend beyond left bed edge: min.x = {}",
            obj.bounds.min.x
        );
        assert!(
            obj.bounds.min.y >= -0.1,
            "Object should not extend beyond top bed edge: min.y = {}",
            obj.bounds.min.y
        );
        assert!(
            obj.bounds.max.x <= bed_w + 0.1,
            "Object should not extend beyond right bed edge: max.x = {} (bed_w = {})",
            obj.bounds.max.x,
            bed_w
        );
        assert!(
            obj.bounds.max.y <= bed_h + 0.1,
            "Object should not extend beyond bottom bed edge: max.y = {} (bed_h = {})",
            obj.bounds.max.y,
            bed_h
        );

        // Check centering
        let center_x = (obj.bounds.min.x + obj.bounds.max.x) / 2.0;
        let center_y = (obj.bounds.min.y + obj.bounds.max.y) / 2.0;
        assert!(
            (center_x - bed_w / 2.0).abs() < 1.0,
            "Expected center_x ~{}, got {}",
            bed_w / 2.0,
            center_x
        );
        assert!(
            (center_y - bed_h / 2.0).abs() < 1.0,
            "Expected center_y ~{}, got {}",
            bed_h / 2.0,
            center_y
        );
    }

    #[test]
    fn import_small_svg_not_upscaled() {
        // SVG smaller than bed (100x100) should not be upscaled (scale capped at 1.0)
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <rect x="10" y="10" width="80" height="80"/>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        let obj = project.find_object(ids[0]).unwrap();

        // Object should be about 80x80 (the rect size), not scaled up
        let w = obj.bounds.max.x - obj.bounds.min.x;
        let h = obj.bounds.max.y - obj.bounds.min.y;
        assert!(w < 100.0, "Small SVG should not be upscaled, width = {}", w);
        assert!(
            h < 100.0,
            "Small SVG should not be upscaled, height = {}",
            h
        );
    }

    fn transformed_object_bounds(object: &ProjectObject) -> Bounds {
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
        let transformed =
            corners.map(|corner| object.transform.apply_around_center(&corner, &center));
        Bounds::new(
            Point2D::new(
                transformed
                    .iter()
                    .map(|point| point.x)
                    .fold(f64::INFINITY, f64::min),
                transformed
                    .iter()
                    .map(|point| point.y)
                    .fold(f64::INFINITY, f64::min),
            ),
            Point2D::new(
                transformed
                    .iter()
                    .map(|point| point.x)
                    .fold(f64::NEG_INFINITY, f64::max),
                transformed
                    .iter()
                    .map(|point| point.y)
                    .fold(f64::NEG_INFINITY, f64::max),
            ),
        )
    }

    #[test]
    fn import_svg_preserves_embedded_png_alongside_vectors() {
        use base64::Engine;

        let png = create_test_png(4, 3);
        let encoded = base64::engine::general_purpose::STANDARD.encode(png);
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
                <rect x="10" y="10" width="20" height="20" fill="none" stroke="black"/>
                <image x="50" y="20" width="40" height="30" href="data:image/png;base64,{encoded}"/>
            </svg>"#
        );
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let ids = import_svg(svg.as_bytes(), &mut project, layer_id).unwrap();
        assert_eq!(
            ids.len(),
            2,
            "the vector and embedded image must both import"
        );
        assert_eq!(project.assets.len(), 1);

        let raster = ids
            .iter()
            .filter_map(|id| project.find_object(*id))
            .find(|object| matches!(object.data, ObjectData::RasterImage { .. }))
            .expect("embedded image object");
        let ObjectData::RasterImage {
            original_width_px,
            original_height_px,
            asset_key,
            ..
        } = &raster.data
        else {
            unreachable!()
        };
        assert_eq!((*original_width_px, *original_height_px), (4, 3));
        assert!(!asset_key.is_empty());

        let scale = 25.4 / 96.0;
        let offset_x = (project.workspace.bed_width_mm - 200.0 * scale) / 2.0;
        let offset_y = (project.workspace.bed_height_mm - 100.0 * scale) / 2.0;
        let visual = transformed_object_bounds(raster);
        assert!((visual.min.x - (offset_x + 50.0 * scale)).abs() < 0.01);
        assert!((visual.min.y - (offset_y + 20.0 * scale)).abs() < 0.01);
        assert!((visual.width() - 40.0 * scale).abs() < 0.01);
        assert!((visual.height() - 30.0 * scale).abs() < 0.01);
    }

    #[test]
    fn import_svg_preserves_embedded_image_rotation() {
        use base64::Engine;

        let png = create_test_png(4, 2);
        let encoded = base64::engine::general_purpose::STANDARD.encode(png);
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
                <image width="40" height="20" transform="translate(100 20) rotate(90)" href="data:image/png;base64,{encoded}"/>
            </svg>"#
        );
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let ids = import_svg(svg.as_bytes(), &mut project, layer_id).unwrap();
        assert_eq!(ids.len(), 1);
        let raster = project.find_object(ids[0]).unwrap();
        assert!(matches!(raster.data, ObjectData::RasterImage { .. }));
        assert!(raster.transform.b.abs() > 0.9);
        assert!(raster.transform.c.abs() > 0.9);

        let scale = 25.4 / 96.0;
        let offset_x = (project.workspace.bed_width_mm - 200.0 * scale) / 2.0;
        let offset_y = (project.workspace.bed_height_mm - 100.0 * scale) / 2.0;
        let visual = transformed_object_bounds(raster);
        assert!((visual.min.x - (offset_x + 80.0 * scale)).abs() < 0.01);
        assert!((visual.min.y - (offset_y + 20.0 * scale)).abs() < 0.01);
        assert!((visual.width() - 20.0 * scale).abs() < 0.01);
        assert!((visual.height() - 40.0 * scale).abs() < 0.01);
    }

    #[test]
    fn import_svg_preserves_embedded_image_aspect_ratio_alignment() {
        use base64::Engine;

        let png = create_test_png(4, 2);
        let encoded = base64::engine::general_purpose::STANDARD.encode(png);
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
                <image x="50" y="20" width="40" height="40" href="data:image/png;base64,{encoded}"/>
            </svg>"#
        );
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let ids = import_svg(svg.as_bytes(), &mut project, layer_id).unwrap();
        let raster = project.find_object(ids[0]).unwrap();

        let scale = 25.4 / 96.0;
        let offset_x = (project.workspace.bed_width_mm - 200.0 * scale) / 2.0;
        let offset_y = (project.workspace.bed_height_mm - 100.0 * scale) / 2.0;
        let visual = transformed_object_bounds(raster);
        assert!((visual.min.x - (offset_x + 50.0 * scale)).abs() < 0.01);
        assert!((visual.min.y - (offset_y + 30.0 * scale)).abs() < 0.01);
        assert!((visual.width() - 40.0 * scale).abs() < 0.01);
        assert!((visual.height() - 20.0 * scale).abs() < 0.01);
    }

    #[test]
    fn import_invalid_svg_returns_error() {
        let bad = b"not an svg at all";
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let result = import_svg(bad, &mut project, layer_id);
        assert!(result.is_err());
    }

    const ENCODED_SVG_BODY: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40mm" height="20mm" viewBox="0 0 40 20">
        <rect x="2" y="3" width="10" height="5" fill="#ff0000"/>
        <rect x="20" y="3" width="5" height="5" fill="#00ff00"/>
        <text x="2" y="15" font-family="sans-serif" font-size="4">Ação · Avó</text>
    </svg>"##;

    fn assert_encoded_svg_matches_utf8(bytes: &[u8]) {
        let mut expected = test_project();
        let layer_id = first_layer_id(&mut expected);
        let expected_ids =
            import_svg(ENCODED_SVG_BODY.as_bytes(), &mut expected, layer_id).unwrap();
        assert_eq!(expected_ids.len(), 3);
        assert!(expected_ids.iter().any(|id| matches!(
            &expected.find_object(*id).unwrap().data,
            ObjectData::Text { content, .. } if content == "Ação · Avó"
        )));

        let mut actual = test_project();
        let layer_id = first_layer_id(&mut actual);
        let actual_ids = import_svg(bytes, &mut actual, layer_id).unwrap();
        assert_eq!(actual_ids.len(), expected_ids.len());
        for (actual_id, expected_id) in actual_ids.iter().zip(&expected_ids) {
            let actual = actual.find_object(*actual_id).unwrap();
            let expected = expected.find_object(*expected_id).unwrap();
            assert_eq!(actual.data, expected.data);
            assert_eq!(actual.bounds, expected.bounds);
            assert_eq!(actual.transform, expected.transform);
            assert_eq!(actual.name, expected.name);
        }
    }

    #[test]
    fn import_svg_utf16_preserves_text_geometry_and_colors() {
        for little_endian in [true, false] {
            for with_bom in [true, false] {
                let xml = format!("<?xml version=\"1.0\" encoding=\"UTF-16\"?>{ENCODED_SVG_BODY}");
                let mut bytes = Vec::new();
                if with_bom {
                    bytes.extend(if little_endian {
                        [0xff, 0xfe]
                    } else {
                        [0xfe, 0xff]
                    });
                }
                for unit in xml.encode_utf16() {
                    bytes.extend(if little_endian {
                        unit.to_le_bytes()
                    } else {
                        unit.to_be_bytes()
                    });
                }
                assert_encoded_svg_matches_utf8(&bytes);
            }
        }
    }

    #[test]
    fn import_svg_declared_legacy_encodings_preserve_accented_text() {
        for declaration in [
            "<?xml version='1.0' encoding='ISO-8859-1'?>",
            "<?xml version=\"1.0\" encoding = \"windows-1252\" standalone='yes'?>",
        ] {
            let xml = format!("{declaration}{ENCODED_SVG_BODY}");
            let bytes: Vec<u8> = xml
                .chars()
                .map(|ch| u8::try_from(ch as u32).unwrap())
                .collect();
            assert_encoded_svg_matches_utf8(&bytes);
        }
    }

    #[test]
    fn import_svg_honors_windows1252_even_when_bytes_are_valid_utf8() {
        let prefix = b"<?xml version='1.0' encoding='windows-1252'?><svg xmlns='http://www.w3.org/2000/svg' width='100' height='100'><text x='0' y='50' font-family='sans-serif'>";
        for (bytes, expected) in [(b"\xc3\xa9".as_slice(), "Ã©"), (b"\x80".as_slice(), "€")] {
            let mut xml = prefix.to_vec();
            xml.extend_from_slice(bytes);
            xml.extend_from_slice(b"</text></svg>");
            let mut project = test_project();
            let layer_id = first_layer_id(&mut project);
            let ids = import_svg(&xml, &mut project, layer_id).unwrap();
            assert_eq!(ids.len(), 1);
            assert!(matches!(
                &project.find_object(ids[0]).unwrap().data,
                ObjectData::Text { content, .. } if content == expected
            ));
        }
    }

    #[test]
    fn import_svg_utf8_bom_preserves_editable_text() {
        let mut bytes = vec![0xef, 0xbb, 0xbf];
        bytes.extend_from_slice(ENCODED_SVG_BODY.as_bytes());
        assert_encoded_svg_matches_utf8(&bytes);
    }

    #[test]
    fn import_svg_encoded_svgz_preserves_editable_text() {
        use std::io::Write;
        for xml in [
            ENCODED_SVG_BODY.to_string(),
            format!("<?xml version='1.0' encoding='windows-1252'?>{ENCODED_SVG_BODY}"),
        ] {
            let bytes = if xml.starts_with("<?xml") {
                xml.chars()
                    .map(|ch| u8::try_from(ch as u32).unwrap())
                    .collect()
            } else {
                xml.into_bytes()
            };
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(&bytes).unwrap();
            assert_encoded_svg_matches_utf8(&encoder.finish().unwrap());
        }
    }

    #[test]
    fn import_svg_invalid_encoding_does_not_change_project() {
        for bytes in [
            b"<?xml version='1.0' encoding='UTF-8'?><svg>\xff</svg>".as_slice(),
            b"<?xml version='1.0' encoding='made-up'?><svg/>".as_slice(),
            b"<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'><title>Caf\xe9</title><rect width='5' height='5'/></svg>".as_slice(),
            b"<?xml version='1.0' encoding='Shift_JIS'?><svg>\x82</svg>".as_slice(),
            b"\xff\xfe<\0s\0v\0g\0>\0\x00\xd8".as_slice(),
            b"\xff\xfe<\0s\0v\0g\0>".as_slice(),
            b"\xfe\xff\0<\0s\0v\0g\0>\xd8\x00".as_slice(),
        ] {
            let mut project = test_project();
            let layer_id = first_layer_id(&mut project);
            let before = project.clone();
            assert!(matches!(import_svg(bytes, &mut project, layer_id), Err(ImportError::ParseError(_))));
            assert_eq!(project, before);
        }
    }

    #[test]
    fn import_png_image_creates_raster_object() {
        // Minimal valid 1x1 red PNG
        let png_bytes = create_test_png(4, 3);
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let obj_id = import_image(&png_bytes, "test.png", None, &mut project, layer_id).unwrap();
        let obj = project.find_object(obj_id).unwrap();

        match &obj.data {
            ObjectData::RasterImage {
                original_width_px,
                original_height_px,
                asset_key,
                adjustments,
                masks,
            } => {
                assert_eq!(*original_width_px, 4);
                assert_eq!(*original_height_px, 3);
                assert!(!asset_key.is_empty());
                assert!(adjustments.is_none());
                assert!(masks.is_empty());
            }
            _ => panic!("Expected RasterImage"),
        }

        // Asset should be stored
        assert_eq!(project.assets.len(), 1);
        assert_eq!(project.assets[0].original_filename, "test.png");
        assert!(project.get_asset_data(project.assets[0].id).is_some());
    }

    #[test]
    fn import_image_composites_transparency_onto_white() {
        let rgba = image::RgbaImage::from_raw(2, 1, vec![0, 0, 0, 0, 0, 0, 0, 255]).unwrap();
        let mut png_bytes = Vec::new();
        image::DynamicImage::ImageRgba8(rgba)
            .write_to(
                &mut std::io::Cursor::new(&mut png_bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        import_image(&png_bytes, "alpha.png", None, &mut project, layer_id).unwrap();

        let stored = project.get_asset_data(project.assets[0].id).unwrap();
        let decoded = image::load_from_memory(stored).unwrap().to_luma8();
        assert_eq!(decoded.get_pixel(0, 0).0[0], 255);
        assert_eq!(decoded.get_pixel(1, 0).0[0], 0);
    }

    #[test]
    fn import_image_centers_on_bed() {
        let png_bytes = create_test_png(100, 100);
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let obj_id = import_image(&png_bytes, "test.png", None, &mut project, layer_id).unwrap();
        let obj = project.find_object(obj_id).unwrap();

        // 100x100 px image on 400x400 mm bed, scale capped at 1.0
        // obj is 100x100 mm, centered at (150, 150)
        let center_x = (obj.bounds.min.x + obj.bounds.max.x) / 2.0;
        let center_y = (obj.bounds.min.y + obj.bounds.max.y) / 2.0;
        assert!(
            (center_x - 200.0).abs() < 0.1,
            "Expected center_x ~200, got {center_x}"
        );
        assert!(
            (center_y - 200.0).abs() < 0.1,
            "Expected center_y ~200, got {center_y}"
        );
    }

    fn png_with_dpi(width: u32, height: u32, pixels_per_metre: u32) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut buf, width, height);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_pixel_dims(Some(png::PixelDimensions {
                xppu: pixels_per_metre,
                yppu: pixels_per_metre,
                unit: png::Unit::Meter,
            }));
            let mut writer = encoder.write_header().unwrap();
            writer
                .write_image_data(&vec![128u8; (width * height) as usize])
                .unwrap();
        }
        buf
    }

    #[test]
    fn import_image_honors_png_dpi_metadata() {
        // 10000 px/metre = 254 DPI = 0.1 mm per pixel.
        let png_bytes = png_with_dpi(200, 100, 10_000);
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let obj_id = import_image(&png_bytes, "scan.png", None, &mut project, layer_id).unwrap();
        let obj = project.find_object(obj_id).unwrap();

        assert!(
            (obj.bounds.width() - 20.0).abs() < 0.01,
            "200px at 254 DPI must import as 20mm, got {}",
            obj.bounds.width()
        );
        assert!(
            (obj.bounds.height() - 10.0).abs() < 0.01,
            "100px at 254 DPI must import as 10mm, got {}",
            obj.bounds.height()
        );
    }

    #[test]
    fn import_image_defaults_to_96_dpi_without_metadata() {
        let png_bytes = create_test_png(96, 96);
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let obj_id = import_image(&png_bytes, "test.png", None, &mut project, layer_id).unwrap();
        let obj = project.find_object(obj_id).unwrap();

        // 96px at the 96 DPI fallback = exactly one inch = 25.4mm.
        assert!(
            (obj.bounds.width() - 25.4).abs() < 0.01,
            "96px without DPI metadata must import as 25.4mm, got {}",
            obj.bounds.width()
        );
    }

    /// Minimal TIFF header + IFD0 carrying XResolution and ResolutionUnit.
    /// Not a decodable image; exercises only the metadata walker.
    fn tiff_blob(little_endian: bool, resolution: u32, unit: u16) -> Vec<u8> {
        let u16b = |v: u16| {
            if little_endian {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            }
        };
        let u32b = |v: u32| {
            if little_endian {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            }
        };
        let mut b = Vec::new();
        b.extend_from_slice(if little_endian {
            &[0x49, 0x49, 0x2A, 0x00]
        } else {
            &[0x4D, 0x4D, 0x00, 0x2A]
        });
        b.extend_from_slice(&u32b(8)); // IFD0 at offset 8
        b.extend_from_slice(&u16b(2)); // two entries
        // Entry: XResolution (282), RATIONAL (5), count 1, value at offset 38
        b.extend_from_slice(&u16b(282));
        b.extend_from_slice(&u16b(5));
        b.extend_from_slice(&u32b(1));
        b.extend_from_slice(&u32b(38));
        // Entry: ResolutionUnit (296), SHORT (3), count 1, inline value
        b.extend_from_slice(&u16b(296));
        b.extend_from_slice(&u16b(3));
        b.extend_from_slice(&u32b(1));
        b.extend_from_slice(&u16b(unit));
        b.extend_from_slice(&u16b(0));
        b.extend_from_slice(&u32b(0)); // next-IFD offset: none
        // offset 38: the rational resolution / 1
        b.extend_from_slice(&u32b(resolution));
        b.extend_from_slice(&u32b(1));
        b
    }

    #[test]
    fn tiff_dpi_reads_resolution_both_endians_and_units() {
        let le = tiff_blob(true, 300, 2);
        assert_eq!(tiff_dpi(&le), Some(300.0));

        let be = tiff_blob(false, 150, 2);
        assert_eq!(tiff_dpi(&be), Some(150.0));

        // Unit 3 = dots per cm: 118 dpcm ≈ 299.72 dpi.
        let cm = tiff_blob(true, 118, 3);
        let dpi = tiff_dpi(&cm).unwrap();
        assert!((dpi - 299.72).abs() < 0.1, "got {dpi}");

        // Unit 1 = no absolute unit: resolution is meaningless.
        assert_eq!(tiff_dpi(&tiff_blob(true, 300, 1)), None);
    }

    #[test]
    fn bmp_dpi_reads_pixels_per_metre() {
        let mut bmp = vec![0u8; 54];
        bmp[0] = b'B';
        bmp[1] = b'M';
        bmp[14..18].copy_from_slice(&40u32.to_le_bytes()); // BITMAPINFOHEADER
        bmp[38..42].copy_from_slice(&11811i32.to_le_bytes()); // ≈300 DPI
        let dpi = bmp_dpi(&bmp).unwrap();
        assert!((dpi - 300.0).abs() < 0.1, "got {dpi}");

        // Zero/negative resolution fields carry no information.
        bmp[38..42].copy_from_slice(&0i32.to_le_bytes());
        assert_eq!(bmp_dpi(&bmp), None);
    }

    /// JPEG built from an optional JFIF APP0 (with the given unit flag and
    /// density) and an EXIF APP1 wrapping a little-endian TIFF block.
    fn jpeg_with_jfif_and_exif(jfif: Option<(u8, u16)>, exif_dpi: u32) -> Vec<u8> {
        let mut jpeg = vec![0xFF, 0xD8];
        if let Some((units, density)) = jfif {
            jpeg.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]);
            jpeg.extend_from_slice(b"JFIF\0");
            jpeg.extend_from_slice(&[0x01, 0x02]); // version
            jpeg.push(units);
            jpeg.extend_from_slice(&density.to_be_bytes());
            jpeg.extend_from_slice(&density.to_be_bytes());
            jpeg.extend_from_slice(&[0, 0]); // no thumbnail
        }
        let tiff = tiff_blob(true, exif_dpi, 2);
        jpeg.extend_from_slice(&[0xFF, 0xE1]);
        jpeg.extend_from_slice(&((2 + 6 + tiff.len()) as u16).to_be_bytes());
        jpeg.extend_from_slice(b"Exif\0\0");
        jpeg.extend_from_slice(&tiff);
        jpeg.extend_from_slice(&[0xFF, 0xD9]);
        jpeg
    }

    #[test]
    fn jpeg_dpi_junk_jfif_density_does_not_shadow_valid_exif() {
        // JFIF claims a degenerate 1 DPI; EXIF carries the real 300 DPI.
        // The junk JFIF value must not short-circuit the scan.
        let jpeg = jpeg_with_jfif_and_exif(Some((1, 1)), 300);
        assert_eq!(jpeg_dpi(&jpeg), Some(300.0));
    }

    #[test]
    fn jpeg_dpi_sane_jfif_density_wins_over_exif() {
        let jpeg = jpeg_with_jfif_and_exif(Some((1, 254)), 300);
        assert_eq!(jpeg_dpi(&jpeg), Some(254.0));
    }

    #[test]
    fn jpeg_dpi_falls_back_to_exif_when_jfif_has_no_units() {
        // SOI + APP1/EXIF wrapping a little-endian TIFF block at 254 DPI.
        let tiff = tiff_blob(true, 254, 2);
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1];
        let payload_len = (2 + 6 + tiff.len()) as u16;
        jpeg.extend_from_slice(&payload_len.to_be_bytes());
        jpeg.extend_from_slice(b"Exif\0\0");
        jpeg.extend_from_slice(&tiff);
        jpeg.extend_from_slice(&[0xFF, 0xD9]);

        assert_eq!(jpeg_dpi(&jpeg), Some(254.0));
    }

    #[test]
    fn import_image_keeps_true_size_when_larger_than_bed() {
        // 4000px at the 96 DPI fallback is 1058.3mm: far larger than the
        // 400mm bed. Dimensions must be preserved (never scaled to fit);
        // the import pipeline warns instead.
        let png_bytes = create_test_png(4000, 100);
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let obj_id = import_image(&png_bytes, "wide.png", None, &mut project, layer_id).unwrap();
        let obj = project.find_object(obj_id).unwrap();

        let expected = 4000.0 * 25.4 / 96.0;
        assert!(
            (obj.bounds.width() - expected).abs() < 0.1,
            "oversized raster must keep true width {expected}, got {}",
            obj.bounds.width()
        );
    }

    #[test]
    fn import_invalid_image_returns_error() {
        let bad = b"not an image";
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let result = import_image(bad, "bad.png", None, &mut project, layer_id);
        assert!(result.is_err());
    }

    #[test]
    fn usvg_segments_to_svg_d_generates_valid_string() {
        use usvg::tiny_skia_path::PathBuilder;
        let mut pb = PathBuilder::new();
        pb.move_to(10.0, 20.0);
        pb.line_to(30.0, 40.0);
        pb.close();
        let path = pb.finish().unwrap();

        let d = usvg_segments_to_svg_d(path.segments());
        assert!(d.contains('M'));
        assert!(d.contains('L'));
        assert!(d.contains('Z'));
    }

    /// Create a minimal valid PNG image of the given dimensions for testing.
    fn create_test_png(width: u32, height: u32) -> Vec<u8> {
        use image::{ImageBuffer, Rgba};
        let img = ImageBuffer::from_pixel(width, height, Rgba([255u8, 0, 0, 255]));
        let mut buf = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut buf);
        img.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
        buf
    }

    #[test]
    fn import_svg_with_text_and_system_font_creates_text_object() {
        // Use sans-serif which always resolves via the generic family fallback
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <text x="10" y="50" font-family="sans-serif" font-size="24">Hello World</text>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        assert!(!ids.is_empty(), "Should create at least one object");

        // Find the text object
        let text_obj = ids.iter().find_map(|&id| {
            let obj = project.find_object(id)?;
            if matches!(&obj.data, ObjectData::Text { .. }) {
                Some(obj)
            } else {
                None
            }
        });
        assert!(text_obj.is_some(), "Should create an editable Text object");
        let text_obj = text_obj.unwrap();
        match &text_obj.data {
            ObjectData::Text {
                content,
                font_family,
                ..
            } => {
                assert_eq!(content, "Hello World");
                assert_eq!(font_family, "sans-serif");
            }
            _ => unreachable!(),
        }
    }

    fn imported_vector_widths(project: &Project, ids: &[ObjectId]) -> Vec<f64> {
        ids.iter()
            .filter_map(|id| project.find_object(*id))
            .filter(|obj| matches!(obj.data, ObjectData::VectorPath { .. }))
            .map(|obj| obj.bounds.width())
            .collect()
    }

    #[test]
    fn import_svg_skips_hidden_vectors() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <rect x="10" y="10" width="20" height="20" visibility="hidden"/>
            <g style="visibility:hidden"><rect x="50" y="50" width="20" height="20"/></g>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);
        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        assert!(imported_vector_widths(&project, &ids).is_empty());
    }

    #[test]
    fn import_svg_applies_clip_paths() {
        let px = 25.4 / 96.0;
        // A filled 20 px square and a stroked 20 px line, both clipped to 10 px.
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <defs><clipPath id="c"><rect x="0" y="0" width="10" height="100"/></clipPath></defs>
            <rect clip-path="url(#c)" x="0" y="0" width="20" height="20" fill="black"/>
            <g clip-path="url(#c)"><path d="M0 50 L20 50" fill="none" stroke="red"/></g>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);
        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        let widths = imported_vector_widths(&project, &ids);
        assert_eq!(widths.len(), 2, "{widths:?}");
        for width in widths {
            assert!((width - 10.0 * px).abs() < 0.01, "{width}");
        }
    }

    #[test]
    fn svg_clipping_respects_subject_and_clip_winding_rules() {
        for clip_rule in ["nonzero", "evenodd"] {
            for subject in ["stroke", "nonzero", "evenodd"] {
                let data = "M0 0 H20 V20 H0 Z M10 0 H30 V20 H10 Z";
                let subject = if subject == "stroke" {
                    r#"<path d="M0 10 H30" fill="none" stroke="red"/>"#.to_string()
                } else {
                    format!(r#"<path d="{data}" fill-rule="{subject}"/>"#)
                };
                let svg = format!(
                    r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
                    <defs><clipPath id="c"><path clip-rule="{clip_rule}" d="{data}"/></clipPath></defs>
                    <g clip-path="url(#c)">{subject}</g></svg>"#
                );
                let mut project = test_project();
                let layer_id = first_layer_id(&mut project);
                let ids = import_svg(svg.as_bytes(), &mut project, layer_id).unwrap();
                let lines: Vec<_> = ids
                    .iter()
                    .filter_map(|id| project.find_object(*id))
                    .filter_map(|object| match &object.data {
                        ObjectData::VectorPath { path_data, .. } => {
                            Some(VecPath::parse_svg_d(path_data))
                        }
                        _ => None,
                    })
                    .flat_map(|path| flatten_vecpath(&path, 0.01))
                    .collect();
                let offset_x = (project.workspace.bed_width_mm - 100.0 * 25.4 / 96.0) / 2.0;
                let overlap_x = offset_x + 15.0 * 25.4 / 96.0;
                let retains_overlap = if subject.contains("stroke=") {
                    lines.iter().any(|line| {
                        line.points.windows(2).any(|p| {
                            p[0].x.min(p[1].x) < overlap_x && p[0].x.max(p[1].x) > overlap_x
                        })
                    })
                } else {
                    // The midpoint of each retained region's bounds distinguishes
                    // a full 30 px union from the two disjoint 10 px strips.
                    lines.iter().any(|line| {
                        let min = line
                            .points
                            .iter()
                            .map(|p| p.x)
                            .fold(f64::INFINITY, f64::min);
                        let max = line
                            .points
                            .iter()
                            .map(|p| p.x)
                            .fold(f64::NEG_INFINITY, f64::max);
                        min < overlap_x && max > overlap_x
                    })
                };
                let expected = clip_rule == "nonzero" && !subject.contains("fill-rule=\"evenodd\"");
                assert_eq!(
                    retains_overlap, expected,
                    "clip {clip_rule}, subject {subject}"
                );
            }
        }
    }

    #[test]
    fn svg_outline_text_uses_bundled_fallback_without_system_fonts() {
        let mut options = usvg::Options::default();
        assert_eq!(options.fontdb.faces().count(), 0);
        ensure_svg_fallback_fonts(options.fontdb_mut());
        for family in ["sans-serif", "serif", "Missing Font"] {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><text x="10" y="50" font-family="{family}" transform="rotate(30)">Turned</text></svg>"#
            );
            let tree = usvg::Tree::from_data(svg.as_bytes(), &options).unwrap();
            let mut groups = Vec::new();
            collect_paths_by_paint(
                tree.root(),
                1.0,
                0.0,
                0.0,
                usvg::Transform::default(),
                None,
                &mut groups,
            );
            assert!(
                !groups.is_empty(),
                "{family} must produce outlines without system fonts"
            );
        }
    }

    #[test]
    fn import_svg_text_follows_viewbox_scale_and_parent_transforms() {
        // 100 mm wide document with a 100 unit viewBox: one unit is 1 mm.
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">
            <g transform="translate(20 0)">
                <text x="10" y="50" font-family="sans-serif" font-size="10">Label</text>
            </g>
            <text x="10" y="80" font-family="sans-serif" font-size="10" transform="rotate(30)">Turned</text>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);
        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        let texts: Vec<_> = ids
            .iter()
            .filter_map(|id| project.find_object(*id))
            .filter_map(|obj| match &obj.data {
                ObjectData::Text {
                    content,
                    font_size_mm,
                    ..
                } => Some((content.clone(), *font_size_mm, obj.bounds.min.x)),
                _ => None,
            })
            .collect();
        assert_eq!(
            texts.len(),
            1,
            "rotated text imports as outlines: {texts:?}"
        );
        assert!(
            !imported_vector_widths(&project, &ids).is_empty(),
            "rotated text outlines are imported, not dropped"
        );
        let (content, size, x) = &texts[0];
        assert_eq!(content, "Label");
        assert!((size - 10.0).abs() < 1e-6, "font size {size}");
        let offset_x = (project.workspace.bed_width_mm - 100.0) / 2.0;
        assert!((x - (offset_x + 30.0)).abs() < 1e-6, "x {x}");
    }

    #[test]
    fn import_svg_does_not_read_local_files_through_image_links() {
        let dir = tempfile::tempdir().unwrap();
        let local = dir.path().join("private.png");
        std::fs::write(&local, create_test_png(2, 2)).unwrap();
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
                <image href="{}" x="0" y="0" width="10" height="10"/>
            </svg>"#,
            local.display()
        );
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);
        import_svg(svg.as_bytes(), &mut project, layer_id).unwrap();
        assert!(project.assets.is_empty());
    }

    #[test]
    fn import_svg_without_text_unchanged_behavior() {
        // SVG with no text elements should behave exactly as before
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <rect x="10" y="10" width="80" height="80"/>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        assert_eq!(ids.len(), 1);

        let obj = project.find_object(ids[0]).unwrap();
        assert!(
            matches!(&obj.data, ObjectData::VectorPath { .. }),
            "SVG without text should produce VectorPath"
        );
    }

    #[test]
    fn import_svg_text_has_correct_font_properties() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <text x="20" y="80" font-family="serif" font-size="18" font-weight="bold" font-style="italic">Styled Text</text>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let ids = import_svg(svg, &mut project, layer_id).unwrap();

        let text_obj = ids.iter().find_map(|&id| {
            let obj = project.find_object(id)?;
            if matches!(&obj.data, ObjectData::Text { .. }) {
                Some(obj)
            } else {
                None
            }
        });
        assert!(text_obj.is_some());
        match &text_obj.unwrap().data {
            ObjectData::Text {
                content,
                font_family,
                bold,
                italic,
                ..
            } => {
                assert_eq!(content, "Styled Text");
                assert_eq!(font_family, "serif");
                assert!(*bold);
                assert!(*italic);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn import_svg_text_parses_text_anchor() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <text x="100" y="50" font-family="sans-serif" font-size="16" text-anchor="middle">Centered</text>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);
        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        let text_obj = ids
            .iter()
            .find_map(|&id| {
                let obj = project.find_object(id)?;
                if matches!(&obj.data, ObjectData::Text { .. }) {
                    Some(obj)
                } else {
                    None
                }
            })
            .expect("Should create text object");
        match &text_obj.data {
            ObjectData::Text { alignment, .. } => {
                assert_eq!(
                    *alignment,
                    TextAlignment::Center,
                    "text-anchor='middle' should map to Center"
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn import_svg_text_parses_text_anchor_end() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <text x="190" y="50" font-family="sans-serif" font-size="16" text-anchor="end">Right</text>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);
        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        let text_obj = ids
            .iter()
            .find_map(|&id| {
                let obj = project.find_object(id)?;
                if matches!(&obj.data, ObjectData::Text { .. }) {
                    Some(obj)
                } else {
                    None
                }
            })
            .expect("Should create text object");
        match &text_obj.data {
            ObjectData::Text { alignment, .. } => {
                assert_eq!(
                    *alignment,
                    TextAlignment::Right,
                    "text-anchor='end' should map to Right"
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn import_svg_text_parses_letter_spacing() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <text x="10" y="50" font-family="sans-serif" font-size="16" letter-spacing="2">Spaced</text>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);
        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        let text_obj = ids
            .iter()
            .find_map(|&id| {
                let obj = project.find_object(id)?;
                if matches!(&obj.data, ObjectData::Text { .. }) {
                    Some(obj)
                } else {
                    None
                }
            })
            .expect("Should create text object");
        match &text_obj.data {
            ObjectData::Text { h_spacing, .. } => {
                assert!(
                    *h_spacing > 0.0,
                    "letter-spacing='2' should produce positive h_spacing, got {h_spacing}"
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn import_svg_text_parses_direction_rtl() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <text x="10" y="50" font-family="sans-serif" font-size="16" direction="rtl">RTL Text</text>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);
        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        let text_obj = ids
            .iter()
            .find_map(|&id| {
                let obj = project.find_object(id)?;
                if matches!(&obj.data, ObjectData::Text { .. }) {
                    Some(obj)
                } else {
                    None
                }
            })
            .expect("Should create text object");
        match &text_obj.data {
            ObjectData::Text { rtl, .. } => {
                assert!(*rtl, "direction='rtl' should set rtl=true");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn import_svg_text_parses_dominant_baseline() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <text x="10" y="100" font-family="sans-serif" font-size="16" dominant-baseline="middle">Middle</text>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);
        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        let text_obj = ids
            .iter()
            .find_map(|&id| {
                let obj = project.find_object(id)?;
                if matches!(&obj.data, ObjectData::Text { .. }) {
                    Some(obj)
                } else {
                    None
                }
            })
            .expect("Should create text object");
        match &text_obj.data {
            ObjectData::Text { alignment_v, .. } => {
                assert_eq!(
                    *alignment_v,
                    TextAlignmentV::Middle,
                    "dominant-baseline='middle' should map to Middle"
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn import_svg_text_default_alignment_is_left() {
        // No text-anchor attribute → should default to Left
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <text x="10" y="50" font-family="sans-serif" font-size="16">Default</text>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);
        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        let text_obj = ids
            .iter()
            .find_map(|&id| {
                let obj = project.find_object(id)?;
                if matches!(&obj.data, ObjectData::Text { .. }) {
                    Some(obj)
                } else {
                    None
                }
            })
            .expect("Should create text object");
        match &text_obj.data {
            ObjectData::Text {
                alignment,
                rtl,
                h_spacing,
                ..
            } => {
                assert_eq!(*alignment, TextAlignment::Left);
                assert!(!*rtl);
                assert!(h_spacing.abs() < f64::EPSILON);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn import_svg_text_parses_style_based_text_anchor() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <text x="100" y="50" style="font-family: sans-serif; font-size: 16px; text-anchor: middle">Styled Center</text>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);
        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        let text_obj = ids
            .iter()
            .find_map(|&id| {
                let obj = project.find_object(id)?;
                if matches!(&obj.data, ObjectData::Text { .. }) {
                    Some(obj)
                } else {
                    None
                }
            })
            .expect("Should create text object");
        match &text_obj.data {
            ObjectData::Text { alignment, .. } => {
                assert_eq!(
                    *alignment,
                    TextAlignment::Center,
                    "style text-anchor should map to Center"
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn import_svg_text_parses_style_based_direction() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <text x="10" y="50" style="font-family: sans-serif; font-size: 16px; direction: rtl">RTL Styled</text>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);
        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        let text_obj = ids
            .iter()
            .find_map(|&id| {
                let obj = project.find_object(id)?;
                if matches!(&obj.data, ObjectData::Text { .. }) {
                    Some(obj)
                } else {
                    None
                }
            })
            .expect("Should create text object");
        match &text_obj.data {
            ObjectData::Text { rtl, .. } => {
                assert!(*rtl, "style direction='rtl' should set rtl=true");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn import_svg_text_parses_style_based_letter_spacing() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <text x="10" y="50" style="font-family: sans-serif; font-size: 16px; letter-spacing: 3px">Spaced Style</text>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);
        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        let text_obj = ids
            .iter()
            .find_map(|&id| {
                let obj = project.find_object(id)?;
                if matches!(&obj.data, ObjectData::Text { .. }) {
                    Some(obj)
                } else {
                    None
                }
            })
            .expect("Should create text object");
        match &text_obj.data {
            ObjectData::Text { h_spacing, .. } => {
                assert!(
                    *h_spacing > 0.0,
                    "style letter-spacing should produce positive h_spacing, got {h_spacing}"
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn import_svg_text_tspan_position_fallback() {
        // Parent <text> has no x/y; first <tspan> provides position
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <text font-family="sans-serif" font-size="16"><tspan x="30" y="60">Positioned Tspan</tspan></text>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);
        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        let text_obj = ids
            .iter()
            .find_map(|&id| {
                let obj = project.find_object(id)?;
                if matches!(&obj.data, ObjectData::Text { .. }) {
                    Some(obj)
                } else {
                    None
                }
            })
            .expect("Should create text object from tspan");
        // Verify the object was positioned using tspan coordinates (scaled)
        assert!(text_obj.bounds.min.x > 0.0, "x should come from tspan x=30");
    }

    #[test]
    fn import_svg_text_style_based_font_properties() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <text x="10" y="50" style="font-family: sans-serif; font-size: 20px; font-weight: bold; font-style: italic">CSS Styled</text>
        </svg>"#;
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);
        let ids = import_svg(svg, &mut project, layer_id).unwrap();
        let text_obj = ids
            .iter()
            .find_map(|&id| {
                let obj = project.find_object(id)?;
                if matches!(&obj.data, ObjectData::Text { .. }) {
                    Some(obj)
                } else {
                    None
                }
            })
            .expect("Should create text object");
        match &text_obj.data {
            ObjectData::Text {
                bold,
                italic,
                font_size_mm,
                ..
            } => {
                assert!(*bold, "style font-weight: bold should set bold=true");
                assert!(*italic, "style font-style: italic should set italic=true");
                // font-size 20px at 96 DPI = 20 * 25.4/96 = 5.29mm. The 200x200
                // SVG fits the 400x400 bed at 1:1, so only px->mm conversion applies.
                assert!(
                    (*font_size_mm - 5.2917).abs() < 0.01,
                    "style font-size: 20px should produce ~5.29mm, got {font_size_mm}"
                );
            }
            _ => unreachable!(),
        }
    }
}

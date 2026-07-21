use thiserror::Error;
use usvg::tiny_skia_path::PathSegment;

use beambench_common::path::{PathCommand, SubPath, VecPath};
use beambench_common::{Bounds, Point2D};

use crate::asset::{Asset, AssetMediaType};
use crate::layer::LayerId;
use crate::object::{
    ObjectData, ObjectId, ProjectObject, TextAlignment, TextAlignmentV, TextLayoutMode,
};
use crate::project::Project;
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

/// Metadata extracted from an SVG `<text>` element before usvg flattening.
struct SvgTextElement {
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

/// Pre-parse SVG XML to extract `<text>` elements with their metadata.
/// Returns extracted text elements and modified SVG bytes with those `<text>` elements removed.
fn extract_svg_text_elements(svg_bytes: &[u8]) -> (Vec<SvgTextElement>, Vec<u8>) {
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

    for node in doc.descendants() {
        if node.tag_name().name() == "text" {
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
                texts.push(SvgTextElement {
                    content,
                    font_family,
                    font_size_px,
                    x,
                    y,
                    bold,
                    italic,
                    alignment,
                    alignment_v,
                    h_spacing_px,
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

/// Import an SVG file into the project.
///
/// First pre-parses XML to extract `<text>` elements. If a text element's font
/// is available on the system, it creates an editable `ObjectData::Text` object.
/// Otherwise, usvg handles the text element (flattening to paths).
///
/// All remaining paths in the SVG are flattened into one combined VectorPath
/// with coordinates transformed to bed-space at true physical size, centered.
pub fn import_svg(
    svg_bytes: &[u8],
    project: &mut Project,
    layer_id: LayerId,
) -> Result<Vec<ObjectId>, ImportError> {
    let mut created_ids = Vec::new();

    // Phase 1: Extract editable text elements
    let (text_elements, remaining_svg) = extract_svg_text_elements(svg_bytes);

    // Phase 2: Parse remaining SVG with usvg for path extraction
    let options = usvg::Options::default();
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

        let obj = ProjectObject::new(
            &format!("Text: {}", text_el.content),
            layer_id,
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

    // Collect paths recursively, transformed to bed space and grouped by
    // paint so differently-colored parts arrive as separate objects (color
    // is how multi-operation designs mark which parts get which settings;
    // a single flattened object made parts unselectable — report #13).
    let mut paint_groups: Vec<PaintGroup> = Vec::new();
    collect_paths_by_paint(tree.root(), scale, offset_x, offset_y, &mut paint_groups);

    // A file with this many distinct paints is color noise (flattened
    // gradient art), not operation intent. Import it as one object rather
    // than exploding the object list.
    const MAX_PAINT_GROUPS: usize = 16;
    if paint_groups.len() > MAX_PAINT_GROUPS {
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

        let obj = ProjectObject::new(
            &name,
            layer_id,
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
    let gray = img.to_luma8();
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

/// Recursively collect all path segments from a usvg group, transforming
/// each point to bed-space coordinates (SVG document → scaled + centered).
/// Paths accumulated per distinct SVG paint (stroke + fill), in first-seen
/// document order. Laser designs conventionally encode operation intent as
/// color (red = cut, black = engrave, ...), so each distinct paint imports as
/// its own selectable object instead of everything flattening into one.
struct PaintGroup {
    /// Identity key: stroke and fill paints, e.g. `s:#FF0000|f:none`.
    key: String,
    /// Hex color used in the object name (stroke preferred, else fill).
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
    groups: &mut Vec<PaintGroup>,
) {
    for node in group.children() {
        match node {
            usvg::Node::Path(path) => {
                let d = transform_path_to_bed_space(path, scale, offset_x, offset_y);
                if d.is_empty() {
                    continue;
                }
                let stroke_hex = paint_hex(path.stroke().map(|s| s.paint()));
                let fill_hex = paint_hex(path.fill().map(|f| f.paint()));
                let key = format!(
                    "s:{}|f:{}",
                    stroke_hex.as_deref().unwrap_or("none"),
                    fill_hex.as_deref().unwrap_or("none")
                );

                let bbox = path.abs_bounding_box();
                let bx0 = bbox.left() as f64 * scale + offset_x;
                let by0 = bbox.top() as f64 * scale + offset_y;
                let bx1 = bbox.right() as f64 * scale + offset_x;
                let by1 = bbox.bottom() as f64 * scale + offset_y;

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

                if d.contains('Z') {
                    entry.has_closed = true;
                }
                if !entry.path_data.is_empty() {
                    entry.path_data.push(' ');
                }
                entry.path_data.push_str(&d);
                entry.min_x = entry.min_x.min(bx0);
                entry.min_y = entry.min_y.min(by0);
                entry.max_x = entry.max_x.max(bx1);
                entry.max_y = entry.max_y.max(by1);
            }
            usvg::Node::Group(g) => {
                collect_paths_by_paint(g, scale, offset_x, offset_y, groups);
            }
            _ => {} // Skip embedded images and text for now
        }
    }
}

/// Convert a single usvg path's segments to an SVG d-string with all
/// coordinates transformed from path-local space → SVG document space → bed space.
fn transform_path_to_bed_space(
    path: &usvg::Path,
    scale: f64,
    offset_x: f64,
    offset_y: f64,
) -> String {
    let t = path.abs_transform();
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

    VecPath { subpaths }.to_svg_d()
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

    #[test]
    fn import_invalid_svg_returns_error() {
        let bad = b"not an svg at all";
        let mut project = test_project();
        let layer_id = first_layer_id(&mut project);

        let result = import_svg(bad, &mut project, layer_id);
        assert!(result.is_err());
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

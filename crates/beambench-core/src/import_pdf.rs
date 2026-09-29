//! PDF vector path extraction and a simplified PostScript AI/EPS extractor.

use beambench_common::{
    geometry::{Point2D, Transform2D},
    path::{PathCommand, SubPath, VecPath},
};

/// PDF/PostScript user-space unit is 1/72 inch (a "point"). Convert to mm.
const PT_TO_MM: f64 = 25.4 / 72.0;

/// An sRGB color recovered from a PDF content stream.
///
/// PDF device-color components are normalized floating point values. They are
/// clamped and rounded to 8-bit channels here so callers can use the color as a
/// stable layer identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfRgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// The PDF painting operation that consumed a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfPaintMode {
    Stroke,
    Fill,
    FillStroke,
    /// Geometry reached the end of a content stream without a paint operator.
    /// This preserves compatibility with path-only fixtures and unusual PDFs.
    Unspecified,
}

/// A PDF path together with the colors in effect when it was painted.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfPaintedPath {
    pub path: VecPath,
    pub stroke_color: Option<PdfRgbColor>,
    pub fill_color: Option<PdfRgbColor>,
    pub paint_mode: PdfPaintMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PdfDeviceColorSpace {
    Gray,
    Rgb,
    Cmyk,
}

#[derive(Debug, Clone, Copy)]
struct PdfGraphicsState {
    stroke_space: PdfDeviceColorSpace,
    fill_space: PdfDeviceColorSpace,
    stroke_color: PdfRgbColor,
    fill_color: PdfRgbColor,
    ctm: Transform2D,
}

impl Default for PdfGraphicsState {
    fn default() -> Self {
        Self {
            stroke_space: PdfDeviceColorSpace::Gray,
            fill_space: PdfDeviceColorSpace::Gray,
            // DeviceGray's initial value is zero (black).
            stroke_color: PdfRgbColor { r: 0, g: 0, b: 0 },
            fill_color: PdfRgbColor { r: 0, g: 0, b: 0 },
            ctm: Transform2D::identity(),
        }
    }
}

fn component_to_u8(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn gray_to_rgb(gray: f64) -> PdfRgbColor {
    let channel = component_to_u8(gray);
    PdfRgbColor {
        r: channel,
        g: channel,
        b: channel,
    }
}

fn device_rgb_to_rgb(red: f64, green: f64, blue: f64) -> PdfRgbColor {
    PdfRgbColor {
        r: component_to_u8(red),
        g: component_to_u8(green),
        b: component_to_u8(blue),
    }
}

fn device_cmyk_to_rgb(cyan: f64, magenta: f64, yellow: f64, black: f64) -> PdfRgbColor {
    let cyan = cyan.clamp(0.0, 1.0);
    let magenta = magenta.clamp(0.0, 1.0);
    let yellow = yellow.clamp(0.0, 1.0);
    let black = black.clamp(0.0, 1.0);
    PdfRgbColor {
        r: component_to_u8(1.0 - (cyan + black).min(1.0)),
        g: component_to_u8(1.0 - (magenta + black).min(1.0)),
        b: component_to_u8(1.0 - (yellow + black).min(1.0)),
    }
}

/// Scale every coordinate of a path by `scale` (used to convert points to mm).
fn scale_vecpath(path: &mut VecPath, scale: f64) {
    for subpath in &mut path.subpaths {
        for command in &mut subpath.commands {
            match command {
                PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => {
                    *x *= scale;
                    *y *= scale;
                }
                PathCommand::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                } => {
                    *c1x *= scale;
                    *c1y *= scale;
                    *c2x *= scale;
                    *c2y *= scale;
                    *x *= scale;
                    *y *= scale;
                }
                _ => {}
            }
        }
    }
}

/// Find the first occurrence of `needle` in `haystack` at or after `from`.
#[cfg(test)]
fn find_bytes(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from > haystack.len() || needle.is_empty() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// Check whether the dictionary (`<<...>>`) immediately preceding the
/// `stream` keyword at `stream_kw_pos` declares `/FlateDecode`.
#[cfg(test)]
fn stream_dict_has_flate(content: &[u8], stream_kw_pos: usize) -> bool {
    // Walk back over whitespace between ">>" and "stream".
    let mut end = stream_kw_pos;
    while end > 0 && content[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    if end < 2 || &content[end - 2..end] != b">>" {
        return false;
    }
    // Balance "<<"/">>" backwards to find the matching opener (dicts nest).
    let mut depth = 0usize;
    let mut j = end;
    while j >= 2 {
        let pair = &content[j - 2..j];
        if pair == b">>" {
            depth += 1;
            j -= 2;
        } else if pair == b"<<" {
            depth -= 1;
            j -= 2;
            if depth == 0 {
                return find_bytes(&content[j..end], b"/FlateDecode", 0).is_some();
            }
        } else {
            j -= 1;
        }
    }
    false
}

/// Decompress a zlib/FlateDecode stream.
#[cfg(test)]
fn flate_decode(data: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(data)
        .take((32 * 1024 * 1024 + 1) as u64)
        .read_to_end(&mut out)
        .map_err(|e| format!("FlateDecode error: {e}"))?;
    if out.len() > 32 * 1024 * 1024 {
        return Err("PDF expanded content limit exceeded".into());
    }
    Ok(out)
}

// Bound source bytes and aggregate expanded page content independently.
const PDF_INPUT_LIMIT: usize = 64 * 1024 * 1024;
const PDF_CONTENT_LIMIT: usize = 32 * 1024 * 1024;

// Eager xref/metadata streams have a smaller limit than page artwork.
const PDF_EAGER_STREAM_LIMIT: usize = 1024 * 1024;

/// Accept graphics state settings that preserve our vector centerlines and
/// device colors. Do not discard opacity, masks, dashes or compositing effects:
/// doing so could turn hidden artwork into laser paths.
fn validate_pdf_graphics_state(
    doc: &lopdf::Document,
    resources: Option<&lopdf::Object>,
    operation: &lopdf::content::Operation,
) -> Result<(), String> {
    let [lopdf::Object::Name(name)] = operation.operands.as_slice() else {
        return Err("PDF gs requires one graphics state resource name".into());
    };
    let label = String::from_utf8_lossy(name);
    let resolve = || -> lopdf::Result<&lopdf::Dictionary> {
        let resources = resources
            .ok_or(lopdf::Error::DictKey("Resources".into()))?
            .as_dict()?;
        let states = doc.get_dict_in_dict(resources, b"ExtGState")?;
        doc.get_dict_in_dict(states, name)
    };
    let state =
        resolve().map_err(|err| format!("Cannot resolve PDF graphics state /{label}: {err}"))?;
    for (key, value) in state.iter() {
        let (_, value) = doc
            .dereference(value)
            .map_err(|err| format!("Cannot resolve PDF graphics state /{label}: {err}"))?;
        // A null dictionary value is equivalent to an absent entry in PDF.
        if matches!(value, lopdf::Object::Null) {
            continue;
        }
        let number = value.as_float().ok().filter(|v| v.is_finite());
        let name = value.as_name().ok();
        let supported = match key.as_slice() {
            b"Type" => name == Some(b"ExtGState"),
            b"CA" | b"ca" => number == Some(1.0),
            b"SMask" => name == Some(b"None"),
            b"BM" => {
                // The first supported blend mode in an array takes precedence.
                let mode = value.as_array().ok().and_then(|array| array.first())
                    .map_or(Ok((None, value)), |first| doc.dereference(first))
                    .ok().and_then(|(_, mode)| mode.as_name().ok());
                matches!(mode, Some(b"Normal" | b"Compatible"))
            }
            // Same centerline semantics as the accepted w/J/j/M operators.
            b"LW" => number.is_some_and(|v| v >= 0.0),
            b"LC" | b"LJ" => value.as_i64().is_ok_and(|v| (0..=2).contains(&v)),
            b"ML" => number.is_some_and(|v| v >= 1.0),
            b"D" => value.as_array().is_ok_and(|array| {
                matches!(array.as_slice(), [pattern, phase]
                    if doc.dereference(pattern).is_ok_and(|(_, p)| p.as_array().is_ok_and(Vec::is_empty))
                    && doc.dereference(phase).is_ok_and(|(_, p)| p.as_float().is_ok_and(|v| v.is_finite() && v >= 0.0)))
            }),
            b"OP" | b"op" => value.as_bool().is_ok_and(|enabled| !enabled),
            b"OPM" => value.as_i64().is_ok_and(|v| (0..=1).contains(&v)),
            b"TR" => name == Some(b"Identity"),
            b"TR2" => matches!(name, Some(b"Identity" | b"Default")),
            b"BG2" | b"UCR2" | b"HT" => name == Some(b"Default"),
            // Rasterization hints do not change the extracted vector geometry.
            b"RI" => matches!(name, Some(b"AbsoluteColorimetric" | b"RelativeColorimetric" | b"Perceptual" | b"Saturation")),
            b"FL" => number.is_some_and(|v| (0.0..=100.0).contains(&v)),
            b"SM" => number.is_some_and(|v| (0.0..=1.0).contains(&v)),
            b"SA" | b"AIS" | b"TK" => value.as_bool().is_ok(),
            _ => false,
        };
        if !supported {
            return Err(format!(
                "PDF graphics state /{label} setting /{} cannot be preserved. Export a vector PDF with full opacity and flatten transparency, masks, dashes, and other effects before importing.",
                String::from_utf8_lossy(key)
            ));
        }
    }
    Ok(())
}

fn decode_artwork_stream(stream: &lopdf::Stream, limit: usize) -> Result<Vec<u8>, String> {
    if stream.dict.has(b"Filter") {
        stream.filters().map_err(|e| e.to_string())?;
    }
    if let Ok(params) = stream.dict.get(b"DecodeParms")
        && !matches!(params, lopdf::Object::Null)
    {
        let params = params
            .as_dict()
            .map_err(|_| "PDF artwork filter parameter arrays are unsupported")?;
        if params
            .get(b"Predictor")
            .is_ok_and(|value| value.as_i64().ok() != Some(1))
        {
            return Err("PDF pixel prediction is only supported in embedded images".into());
        }
    }
    stream
        .get_plain_content_with_limit(limit)
        .map_err(|e| format!("PDF stream content limit or decoding error: {e}"))
}

mod artwork;
mod objects;
pub use artwork::{PdfArtwork, PdfImage};

/// Resolve page content through the PDF object graph. Unsupported painting is
/// rejected rather than silently importing a different design.
pub fn parse_pdf_artwork(content: &[u8]) -> Result<PdfArtwork, String> {
    if content.len() > PDF_INPUT_LIMIT {
        return Err("PDF exceeds the 64 MiB input limit".into());
    }
    // Reject encryption before any automatic decryption or metadata expansion.
    // The reader exposes the trailer/xref first and lets us defer ObjStm bodies.
    let mut doc = lopdf::Reader {
        buffer: content,
        document: lopdf::Document::new(),
        encryption_state: None,
        raw_objects: Default::default(),
        password: None,
        strict: true,
        max_decompressed_size: Some(PDF_EAGER_STREAM_LIMIT),
    }
    .read_unencrypted(
        Some(|id, object| {
            if let Ok(stream) = object.as_stream_mut()
                && stream.dict.has_type(b"ObjStm")
            {
                stream.dict.set("Type", "BeamBenchDeferredObjects");
            }
            Some((id, object.clone()))
        }),
        100_000,
    )
    .map_err(|err| format!("Cannot read PDF: {err}"))?;
    let mut remaining = PDF_CONTENT_LIMIT;
    objects::expand(&mut doc, &mut remaining)?;
    let pages = doc.get_pages();
    if pages.len() != 1 {
        return Err(
            "Import a single-page PDF. Split this document into separate pages first.".into(),
        );
    }
    let page_id = *pages.values().next().unwrap();
    let mut ancestor = Some(page_id);
    let mut resources = None;
    let mut media_box = None;
    let mut visited = std::collections::HashSet::new();
    while let Some(id) = ancestor {
        if !visited.insert(id) {
            return Err("PDF contains a cyclic page tree".into());
        }
        let page = doc
            .get_object(id)
            .and_then(lopdf::Object::as_dict)
            .map_err(|err| err.to_string())?;
        // Resources are inherited as a whole from the nearest page-tree entry,
        // rather than merging names from different ancestors.
        if resources.is_none()
            && let Ok(value) = page.get(b"Resources")
        {
            let (_, value) = doc.dereference(value).map_err(|err| err.to_string())?;
            if !matches!(value, lopdf::Object::Null) {
                resources = Some(value);
            }
        }
        if media_box.is_none()
            && let Ok(value) = page.get(b"MediaBox")
        {
            let (_, value) = doc.dereference(value).map_err(|e| e.to_string())?;
            let values = artwork::numbers(value.as_array().map_err(|e| e.to_string())?, 4)?;
            if values[2] <= values[0] || values[3] <= values[1] {
                return Err("Invalid PDF MediaBox".into());
            }
            media_box = Some([values[0], values[1], values[2], values[3]]);
        }
        if page
            .get(b"Rotate")
            .is_ok_and(|value| value.as_i64().unwrap_or(1) % 360 != 0)
            || page
                .get(b"UserUnit")
                .is_ok_and(|value| value.as_float().unwrap_or(0.0) != 1.0)
            || page.has(b"CropBox")
        {
            return Err("PDF page rotation, custom units, or cropping cannot be preserved. Export an uncropped, unrotated vector page first.".into());
        }
        ancestor = page
            .get(b"Parent")
            .ok()
            .and_then(|value| value.as_reference().ok());
    }
    let mut decoded = Vec::new();
    for id in doc.get_page_contents(page_id) {
        if decoded.len() >= PDF_CONTENT_LIMIT {
            return Err("PDF expanded content limit exceeded".into());
        }
        let stream = doc
            .get_object(id)
            .and_then(lopdf::Object::as_stream)
            .map_err(|err| err.to_string())?;
        let available = remaining.saturating_sub(1);
        let bytes = decode_artwork_stream(stream, available)?;
        remaining = remaining
            .checked_sub(bytes.len() + 1)
            .ok_or("PDF expanded content limit exceeded")?;
        decoded.extend_from_slice(&bytes);
        decoded.push(b'\n');
    }
    artwork::extract(
        &doc,
        resources,
        &decoded,
        remaining,
        media_box.ok_or("Missing PDF MediaBox")?,
    )
}

/// Vector-only compatibility API. Never silently discard embedded images.
pub fn parse_pdf_painted_paths(content: &[u8]) -> Result<Vec<PdfPaintedPath>, String> {
    let artwork = parse_pdf_artwork(content)?;
    if !artwork.images.is_empty() {
        return Err(
            "PDF contains embedded images (Do); use the mixed-artwork importer to preserve them"
                .into(),
        );
    }
    Ok(artwork.paths)
}

#[cfg(test)]
fn parse_pdf_stream_fixture(content: &[u8]) -> Result<Vec<PdfPaintedPath>, String> {
    let mut paths = Vec::new();

    let mut start = 0;
    while let Some(kw_pos) = find_bytes(content, b"stream", start) {
        // Skip matches that are actually the tail of "endstream".
        if kw_pos >= 3 && &content[kw_pos - 3..kw_pos] == b"end" {
            start = kw_pos + 6;
            continue;
        }
        // The keyword must be followed by CRLF or LF (PDF spec).
        let after = &content[kw_pos + 6..];
        let data_start = if after.starts_with(b"\r\n") {
            kw_pos + 8
        } else if after.starts_with(b"\n") {
            kw_pos + 7
        } else {
            start = kw_pos + 6;
            continue;
        };

        let Some(stream_end) = find_bytes(content, b"endstream", data_start) else {
            break;
        };
        let stream_data = &content[data_start..stream_end];

        let decoded: Vec<u8> = if stream_dict_has_flate(content, kw_pos) {
            match flate_decode(stream_data) {
                Ok(bytes) => bytes,
                Err(_) => {
                    // Undecodable stream (e.g. extra filters) — skip it.
                    start = stream_end + 9;
                    continue;
                }
            }
        } else {
            stream_data.to_vec()
        };

        let text = String::from_utf8_lossy(&decoded);
        for mut painted_path in parse_content_stream(&text) {
            scale_vecpath(&mut painted_path.path, PT_TO_MM);
            paths.push(painted_path);
        }
        start = stream_end + 9; // past "endstream"
    }

    if paths.is_empty() {
        Err("No paths found in PDF".to_string())
    } else {
        Ok(paths)
    }
}

/// Extract geometry from PDF content without exposing its paint metadata.
///
/// This compatibility wrapper retains the original API. New import code should
/// use [`parse_pdf_painted_paths`] so stroke and fill colors are not lost.
pub fn parse_pdf_paths(content: &[u8]) -> Result<Vec<VecPath>, String> {
    parse_pdf_painted_paths(content).map(|paths| paths.into_iter().map(|path| path.path).collect())
}

/// Extract path operators from a PDF content stream string.
fn parse_content_stream(stream: &str) -> Vec<PdfPaintedPath> {
    let tokens = tokenize_pdf_stream(stream);
    let mut painted_paths = Vec::new();
    let mut subpaths = Vec::new();
    let mut current = SubPath::new();
    let mut operands: Vec<String> = Vec::new();
    let mut graphics_state = PdfGraphicsState::default();
    let mut graphics_stack: Vec<PdfGraphicsState> = Vec::new();

    for token in tokens {
        if token.parse::<f64>().is_ok() || token.starts_with('/') {
            operands.push(token);
            continue;
        }

        match token.as_str() {
            "m" => {
                // moveto: x y m
                if let Some([x, y]) = last_numbers::<2>(&operands) {
                    if !current.commands.is_empty() {
                        subpaths.push(current);
                        current = SubPath::new();
                    }
                    let (x, y) = transform_pdf_point(graphics_state.ctm, x, y);
                    current.commands.push(PathCommand::MoveTo { x, y });
                }
            }
            "l" => {
                // lineto: x y l
                if let Some([x, y]) = last_numbers::<2>(&operands) {
                    let (x, y) = transform_pdf_point(graphics_state.ctm, x, y);
                    current.commands.push(PathCommand::LineTo { x, y });
                }
            }
            "c" => {
                // curveto: x1 y1 x2 y2 x3 y3 c
                if let Some([c1x, c1y, c2x, c2y, x, y]) = last_numbers::<6>(&operands) {
                    let (c1x, c1y) = transform_pdf_point(graphics_state.ctm, c1x, c1y);
                    let (c2x, c2y) = transform_pdf_point(graphics_state.ctm, c2x, c2y);
                    let (x, y) = transform_pdf_point(graphics_state.ctm, x, y);
                    current.commands.push(PathCommand::CubicTo {
                        c1x,
                        c1y,
                        c2x,
                        c2y,
                        x,
                        y,
                    });
                }
            }
            "v" => {
                // curveto shorthand: the current point is the first control
                // point, followed by x2 y2 x3 y3 v.
                if let (Some((c1x, c1y)), Some([c2x, c2y, x, y])) = (
                    current_subpath_point(&current),
                    last_numbers::<4>(&operands),
                ) {
                    let (c2x, c2y) = transform_pdf_point(graphics_state.ctm, c2x, c2y);
                    let (x, y) = transform_pdf_point(graphics_state.ctm, x, y);
                    current.commands.push(PathCommand::CubicTo {
                        c1x,
                        c1y,
                        c2x,
                        c2y,
                        x,
                        y,
                    });
                }
            }
            "y" => {
                // curveto shorthand: x1 y1 x3 y3 y, with the endpoint also
                // serving as the second control point.
                if let Some([c1x, c1y, x, y]) = last_numbers::<4>(&operands) {
                    let (c1x, c1y) = transform_pdf_point(graphics_state.ctm, c1x, c1y);
                    let (x, y) = transform_pdf_point(graphics_state.ctm, x, y);
                    current.commands.push(PathCommand::CubicTo {
                        c1x,
                        c1y,
                        c2x: x,
                        c2y: y,
                        x,
                        y,
                    });
                }
            }
            "re" => {
                // rectangle: x y w h re
                if let Some([x, y, w, h]) = last_numbers::<4>(&operands) {
                    if !current.commands.is_empty() {
                        subpaths.push(current);
                        current = SubPath::new();
                    }
                    let p0 = transform_pdf_point(graphics_state.ctm, x, y);
                    let p1 = transform_pdf_point(graphics_state.ctm, x + w, y);
                    let p2 = transform_pdf_point(graphics_state.ctm, x + w, y + h);
                    let p3 = transform_pdf_point(graphics_state.ctm, x, y + h);
                    current
                        .commands
                        .push(PathCommand::MoveTo { x: p0.0, y: p0.1 });
                    current
                        .commands
                        .push(PathCommand::LineTo { x: p1.0, y: p1.1 });
                    current
                        .commands
                        .push(PathCommand::LineTo { x: p2.0, y: p2.1 });
                    current
                        .commands
                        .push(PathCommand::LineTo { x: p3.0, y: p3.1 });
                    current.commands.push(PathCommand::Close);
                    current.closed = true;
                }
            }
            "h" => {
                // closepath
                close_current_subpath(&mut current);
            }
            "q" => graphics_stack.push(graphics_state),
            "Q" => {
                if let Some(saved) = graphics_stack.pop() {
                    graphics_state = saved;
                }
            }
            "cm" => {
                if let Some([a, b, c, d, tx, ty]) = last_numbers::<6>(&operands) {
                    let matrix = Transform2D { a, b, c, d, tx, ty };
                    // PDF concatenates the new matrix inside the existing CTM:
                    // parent transforms therefore continue to apply outside
                    // transforms established by nested content.
                    graphics_state.ctm = graphics_state.ctm.compose(&matrix);
                }
            }
            "G" => {
                if let Some([gray]) = last_numbers::<1>(&operands) {
                    graphics_state.stroke_space = PdfDeviceColorSpace::Gray;
                    graphics_state.stroke_color = gray_to_rgb(gray);
                }
            }
            "g" => {
                if let Some([gray]) = last_numbers::<1>(&operands) {
                    graphics_state.fill_space = PdfDeviceColorSpace::Gray;
                    graphics_state.fill_color = gray_to_rgb(gray);
                }
            }
            "RG" => {
                if let Some([red, green, blue]) = last_numbers::<3>(&operands) {
                    graphics_state.stroke_space = PdfDeviceColorSpace::Rgb;
                    graphics_state.stroke_color = device_rgb_to_rgb(red, green, blue);
                }
            }
            "rg" => {
                if let Some([red, green, blue]) = last_numbers::<3>(&operands) {
                    graphics_state.fill_space = PdfDeviceColorSpace::Rgb;
                    graphics_state.fill_color = device_rgb_to_rgb(red, green, blue);
                }
            }
            "K" => {
                if let Some([cyan, magenta, yellow, black]) = last_numbers::<4>(&operands) {
                    graphics_state.stroke_space = PdfDeviceColorSpace::Cmyk;
                    graphics_state.stroke_color = device_cmyk_to_rgb(cyan, magenta, yellow, black);
                }
            }
            "k" => {
                if let Some([cyan, magenta, yellow, black]) = last_numbers::<4>(&operands) {
                    graphics_state.fill_space = PdfDeviceColorSpace::Cmyk;
                    graphics_state.fill_color = device_cmyk_to_rgb(cyan, magenta, yellow, black);
                }
            }
            "CS" => {
                if let Some(space) = operands
                    .last()
                    .and_then(|name| parse_device_color_space(name))
                {
                    graphics_state.stroke_space = space;
                    graphics_state.stroke_color = default_color_for_space(space);
                }
            }
            "cs" => {
                if let Some(space) = operands
                    .last()
                    .and_then(|name| parse_device_color_space(name))
                {
                    graphics_state.fill_space = space;
                    graphics_state.fill_color = default_color_for_space(space);
                }
            }
            "SC" | "SCN" => {
                if let Some(color) = color_from_operands(&operands, graphics_state.stroke_space) {
                    graphics_state.stroke_color = color;
                }
            }
            "sc" | "scn" => {
                if let Some(color) = color_from_operands(&operands, graphics_state.fill_space) {
                    graphics_state.fill_color = color;
                }
            }
            "S" => finish_painted_path(
                &mut painted_paths,
                &mut subpaths,
                &mut current,
                graphics_state,
                PdfPaintMode::Stroke,
            ),
            "s" => {
                close_current_subpath(&mut current);
                finish_painted_path(
                    &mut painted_paths,
                    &mut subpaths,
                    &mut current,
                    graphics_state,
                    PdfPaintMode::Stroke,
                );
            }
            "f" | "F" | "f*" => {
                close_all_subpaths(&mut subpaths, &mut current);
                finish_painted_path(
                    &mut painted_paths,
                    &mut subpaths,
                    &mut current,
                    graphics_state,
                    PdfPaintMode::Fill,
                );
            }
            "B" | "B*" => {
                // Filling implicitly closes every open subpath. A single
                // VecPath cannot express the open stroke and closed fill views
                // separately, so retain the closed geometry that represents
                // the complete painted shape.
                close_all_subpaths(&mut subpaths, &mut current);
                finish_painted_path(
                    &mut painted_paths,
                    &mut subpaths,
                    &mut current,
                    graphics_state,
                    PdfPaintMode::FillStroke,
                );
            }
            "b" | "b*" => {
                close_all_subpaths(&mut subpaths, &mut current);
                finish_painted_path(
                    &mut painted_paths,
                    &mut subpaths,
                    &mut current,
                    graphics_state,
                    PdfPaintMode::FillStroke,
                );
            }
            "n" => discard_current_path(&mut subpaths, &mut current),
            _ => {}
        }
        operands.clear();
    }

    // Some generated fixtures and malformed-but-useful files contain geometry
    // without a final paint operator. Keep that geometry available, but make
    // the absence of paint metadata explicit.
    finish_painted_path(
        &mut painted_paths,
        &mut subpaths,
        &mut current,
        graphics_state,
        PdfPaintMode::Unspecified,
    );

    painted_paths
}

fn last_numbers<const N: usize>(operands: &[String]) -> Option<[f64; N]> {
    let tail = operands.get(operands.len().checked_sub(N)?..)?;
    let mut values = [0.0; N];
    for (value, token) in values.iter_mut().zip(tail) {
        *value = token.parse().ok()?;
    }
    Some(values)
}

fn transform_pdf_point(transform: Transform2D, x: f64, y: f64) -> (f64, f64) {
    let point = transform.apply(&Point2D::new(x, y));
    (point.x, point.y)
}

fn current_subpath_point(subpath: &SubPath) -> Option<(f64, f64)> {
    match subpath.commands.last()? {
        PathCommand::MoveTo { x, y }
        | PathCommand::LineTo { x, y }
        | PathCommand::QuadTo { x, y, .. }
        | PathCommand::CubicTo { x, y, .. } => Some((*x, *y)),
        PathCommand::Close => subpath.commands.iter().find_map(|command| match command {
            PathCommand::MoveTo { x, y } => Some((*x, *y)),
            _ => None,
        }),
    }
}

fn parse_device_color_space(name: &str) -> Option<PdfDeviceColorSpace> {
    match name {
        "/DeviceGray" => Some(PdfDeviceColorSpace::Gray),
        "/DeviceRGB" => Some(PdfDeviceColorSpace::Rgb),
        "/DeviceCMYK" => Some(PdfDeviceColorSpace::Cmyk),
        _ => None,
    }
}

fn default_color_for_space(space: PdfDeviceColorSpace) -> PdfRgbColor {
    match space {
        PdfDeviceColorSpace::Gray | PdfDeviceColorSpace::Rgb | PdfDeviceColorSpace::Cmyk => {
            PdfRgbColor { r: 0, g: 0, b: 0 }
        }
    }
}

fn color_from_operands(operands: &[String], space: PdfDeviceColorSpace) -> Option<PdfRgbColor> {
    match space {
        PdfDeviceColorSpace::Gray => {
            let [gray] = last_numbers::<1>(operands)?;
            Some(gray_to_rgb(gray))
        }
        PdfDeviceColorSpace::Rgb => {
            let [red, green, blue] = last_numbers::<3>(operands)?;
            Some(device_rgb_to_rgb(red, green, blue))
        }
        PdfDeviceColorSpace::Cmyk => {
            let [cyan, magenta, yellow, black] = last_numbers::<4>(operands)?;
            Some(device_cmyk_to_rgb(cyan, magenta, yellow, black))
        }
    }
}

fn close_current_subpath(current: &mut SubPath) {
    if !current.commands.is_empty() && !current.closed {
        current.commands.push(PathCommand::Close);
        current.closed = true;
    }
}

fn close_all_subpaths(subpaths: &mut [SubPath], current: &mut SubPath) {
    for subpath in subpaths {
        close_current_subpath(subpath);
    }
    close_current_subpath(current);
}

fn take_current_path(subpaths: &mut Vec<SubPath>, current: &mut SubPath) -> Option<VecPath> {
    if !current.commands.is_empty() {
        subpaths.push(std::mem::take(current));
    }
    if subpaths.is_empty() {
        None
    } else {
        Some(VecPath {
            subpaths: std::mem::take(subpaths),
        })
    }
}

fn finish_painted_path(
    painted_paths: &mut Vec<PdfPaintedPath>,
    subpaths: &mut Vec<SubPath>,
    current: &mut SubPath,
    graphics_state: PdfGraphicsState,
    paint_mode: PdfPaintMode,
) {
    let Some(path) = take_current_path(subpaths, current) else {
        return;
    };
    let (stroke_color, fill_color) = match paint_mode {
        PdfPaintMode::Stroke => (Some(graphics_state.stroke_color), None),
        PdfPaintMode::Fill => (None, Some(graphics_state.fill_color)),
        PdfPaintMode::FillStroke => (
            Some(graphics_state.stroke_color),
            Some(graphics_state.fill_color),
        ),
        PdfPaintMode::Unspecified => (None, None),
    };
    painted_paths.push(PdfPaintedPath {
        path,
        stroke_color,
        fill_color,
        paint_mode,
    });
}

fn discard_current_path(subpaths: &mut Vec<SubPath>, current: &mut SubPath) {
    subpaths.clear();
    *current = SubPath::new();
}

/// Tokenize PDF content stream into words/numbers.
fn tokenize_pdf_stream(stream: &str) -> Vec<String> {
    stream
        .lines()
        .flat_map(|line| {
            line.split_once('%')
                .map_or(line, |(code, _)| code)
                .split_whitespace()
        })
        .map(str::to_string)
        .collect()
}

/// Parse EPS file (PostScript) for path commands.
pub fn parse_eps_paths(content: &[u8]) -> Result<Vec<VecPath>, String> {
    let text = String::from_utf8_lossy(content);

    // EPS uses PostScript operators: moveto, lineto, curveto, closepath
    let tokens = tokenize_pdf_stream(&text);
    let mut subpaths = Vec::new();
    let mut current = SubPath::new();
    let mut i = 0;

    while i < tokens.len() {
        match tokens[i].as_str() {
            "moveto" => {
                if i >= 2
                    && let (Ok(x), Ok(y)) =
                        (tokens[i - 2].parse::<f64>(), tokens[i - 1].parse::<f64>())
                {
                    if !current.commands.is_empty() {
                        subpaths.push(current);
                        current = SubPath::new();
                    }
                    current.commands.push(PathCommand::MoveTo { x, y });
                }
            }
            "lineto" => {
                if i >= 2
                    && let (Ok(x), Ok(y)) =
                        (tokens[i - 2].parse::<f64>(), tokens[i - 1].parse::<f64>())
                {
                    current.commands.push(PathCommand::LineTo { x, y });
                }
            }
            "curveto" => {
                if i >= 6
                    && let (Ok(c1x), Ok(c1y), Ok(c2x), Ok(c2y), Ok(x), Ok(y)) = (
                        tokens[i - 6].parse::<f64>(),
                        tokens[i - 5].parse::<f64>(),
                        tokens[i - 4].parse::<f64>(),
                        tokens[i - 3].parse::<f64>(),
                        tokens[i - 2].parse::<f64>(),
                        tokens[i - 1].parse::<f64>(),
                    )
                {
                    current.commands.push(PathCommand::CubicTo {
                        c1x,
                        c1y,
                        c2x,
                        c2y,
                        x,
                        y,
                    });
                }
            }
            "closepath" => {
                current.commands.push(PathCommand::Close);
                current.closed = true;
            }
            _ => {}
        }
        i += 1;
    }

    if !current.commands.is_empty() {
        subpaths.push(current);
    }

    if subpaths.is_empty() {
        Err("No paths found in EPS".to_string())
    } else {
        let mut path = VecPath { subpaths };
        scale_vecpath(&mut path, PT_TO_MM);
        Ok(vec![path])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse_pdf_paths_fixture(content: &[u8]) -> Result<Vec<VecPath>, String> {
        parse_pdf_stream_fixture(content)
            .map(|paths| paths.into_iter().map(|path| path.path).collect())
    }

    fn assert_pdf_coord(actual_mm: f64, expected_points: f64) {
        let expected_mm = expected_points * PT_TO_MM;
        assert!(
            (actual_mm - expected_mm).abs() < 1e-9,
            "expected {expected_mm}mm, got {actual_mm}mm"
        );
    }

    fn assert_command_endpoint(command: &PathCommand, x_points: f64, y_points: f64) {
        match *command {
            PathCommand::MoveTo { x, y }
            | PathCommand::LineTo { x, y }
            | PathCommand::QuadTo { x, y, .. }
            | PathCommand::CubicTo { x, y, .. } => {
                assert_pdf_coord(x, x_points);
                assert_pdf_coord(y, y_points);
            }
            PathCommand::Close => panic!("expected command with an endpoint"),
        }
    }

    #[test]
    fn parse_pdf_moveto_lineto() {
        let content = b"stream\n10 20 m 30 40 l\nendstream";
        let paths = parse_pdf_paths_fixture(content).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].subpaths[0].commands.len(), 2);
    }

    #[test]
    fn parse_pdf_rectangle() {
        let content = b"stream\n10 20 50 30 re\nendstream";
        let paths = parse_pdf_paths_fixture(content).unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].subpaths[0].closed);
        assert_eq!(paths[0].subpaths[0].commands.len(), 5); // M L L L Z
    }

    #[test]
    fn parse_pdf_curveto() {
        let content = b"stream\n0 0 m 10 20 30 40 50 60 c\nendstream";
        let paths = parse_pdf_paths_fixture(content).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].subpaths[0].commands.len(), 2); // M C
    }

    #[test]
    fn parse_pdf_curve_shorthands_preserve_implicit_controls() {
        let content = b"stream\n1 2 m 3 4 5 6 v 7 8 9 10 y S\nendstream";
        let paths = parse_pdf_paths_fixture(content).unwrap();
        let commands = &paths[0].subpaths[0].commands;

        assert_eq!(commands.len(), 3);
        match commands[1] {
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                assert_pdf_coord(c1x, 1.0);
                assert_pdf_coord(c1y, 2.0);
                assert_pdf_coord(c2x, 3.0);
                assert_pdf_coord(c2y, 4.0);
                assert_pdf_coord(x, 5.0);
                assert_pdf_coord(y, 6.0);
            }
            _ => panic!("expected v to produce CubicTo"),
        }
        match commands[2] {
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                assert_pdf_coord(c1x, 7.0);
                assert_pdf_coord(c1y, 8.0);
                assert_pdf_coord(c2x, 9.0);
                assert_pdf_coord(c2y, 10.0);
                assert_pdf_coord(x, 9.0);
                assert_pdf_coord(y, 10.0);
            }
            _ => panic!("expected y to produce CubicTo"),
        }
    }

    #[test]
    fn parse_pdf_ctm_transforms_all_curve_coordinates() {
        let content = b"stream\n2 0 0 3 10 20 cm 1 2 m 4 5 l 6 7 8 9 10 11 c 12 13 14 15 v 16 17 18 19 y S\nendstream";
        let paths = parse_pdf_paths_fixture(content).unwrap();
        let commands = &paths[0].subpaths[0].commands;

        assert_eq!(commands.len(), 5);
        assert_command_endpoint(&commands[0], 12.0, 26.0);
        assert_command_endpoint(&commands[1], 18.0, 35.0);
        match commands[2] {
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                assert_pdf_coord(c1x, 22.0);
                assert_pdf_coord(c1y, 41.0);
                assert_pdf_coord(c2x, 26.0);
                assert_pdf_coord(c2y, 47.0);
                assert_pdf_coord(x, 30.0);
                assert_pdf_coord(y, 53.0);
            }
            _ => panic!("expected CubicTo"),
        }
        match commands[3] {
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                assert_pdf_coord(c1x, 30.0);
                assert_pdf_coord(c1y, 53.0);
                assert_pdf_coord(c2x, 34.0);
                assert_pdf_coord(c2y, 59.0);
                assert_pdf_coord(x, 38.0);
                assert_pdf_coord(y, 65.0);
            }
            _ => panic!("expected v CubicTo"),
        }
        match commands[4] {
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                assert_pdf_coord(c1x, 42.0);
                assert_pdf_coord(c1y, 71.0);
                assert_pdf_coord(c2x, 46.0);
                assert_pdf_coord(c2y, 77.0);
                assert_pdf_coord(x, 46.0);
                assert_pdf_coord(y, 77.0);
            }
            _ => panic!("expected y CubicTo"),
        }
    }

    #[test]
    fn parse_pdf_ctm_concatenates_and_restores_with_graphics_state() {
        let content =
            b"stream\n1 0 0 1 10 20 cm q 2 0 0 3 0 0 cm 1 1 m 2 2 l S Q 1 1 m 2 2 l S\nendstream";
        let paths = parse_pdf_paths_fixture(content).unwrap();

        assert_eq!(paths.len(), 2);
        assert_command_endpoint(&paths[0].subpaths[0].commands[0], 12.0, 23.0);
        assert_command_endpoint(&paths[0].subpaths[0].commands[1], 14.0, 26.0);
        assert_command_endpoint(&paths[1].subpaths[0].commands[0], 11.0, 21.0);
        assert_command_endpoint(&paths[1].subpaths[0].commands[1], 12.0, 22.0);
    }

    #[test]
    fn parse_pdf_ctm_transforms_rectangle_corners() {
        let content = b"stream\n0 1 -1 0 100 200 cm 10 20 30 40 re f\nendstream";
        let paths = parse_pdf_paths_fixture(content).unwrap();
        let commands = &paths[0].subpaths[0].commands;

        assert_eq!(commands.len(), 5);
        assert_command_endpoint(&commands[0], 80.0, 210.0);
        assert_command_endpoint(&commands[1], 80.0, 240.0);
        assert_command_endpoint(&commands[2], 40.0, 240.0);
        assert_command_endpoint(&commands[3], 40.0, 210.0);
        assert_eq!(commands[4], PathCommand::Close);
    }

    #[test]
    fn parse_pdf_separates_painted_paths_and_preserves_rgb_colors() {
        let content = b"stream\n1 0 0 RG 0 0 m 72 0 l S 0 0 1 rg 0 10 72 20 re f\nendstream";
        let paths = parse_pdf_stream_fixture(content).unwrap();

        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].paint_mode, PdfPaintMode::Stroke);
        assert_eq!(
            paths[0].stroke_color,
            Some(PdfRgbColor { r: 255, g: 0, b: 0 })
        );
        assert_eq!(paths[0].fill_color, None);
        assert_eq!(paths[1].paint_mode, PdfPaintMode::Fill);
        assert_eq!(paths[1].stroke_color, None);
        assert_eq!(
            paths[1].fill_color,
            Some(PdfRgbColor { r: 0, g: 0, b: 255 })
        );
        assert!(
            paths[1].path.subpaths[0].closed,
            "PDF fill must materialize its implicit subpath closure"
        );
        assert_eq!(
            paths[1].path.subpaths[0].commands.last(),
            Some(&PathCommand::Close)
        );
        match paths[0].path.subpaths[0].commands[1] {
            PathCommand::LineTo { x, .. } => assert!((x - 25.4).abs() < 1e-6),
            _ => panic!("expected LineTo"),
        }
    }

    #[test]
    fn parse_pdf_gray_and_cmyk_are_clamped_and_converted() {
        let content = b"stream\n1.5 G 0 0 m 1 1 l S 0 1 1 0 k 2 2 3 3 re f\nendstream";
        let paths = parse_pdf_stream_fixture(content).unwrap();

        assert_eq!(
            paths[0].stroke_color,
            Some(PdfRgbColor {
                r: 255,
                g: 255,
                b: 255
            })
        );
        assert_eq!(
            paths[1].fill_color,
            Some(PdfRgbColor { r: 255, g: 0, b: 0 })
        );
    }

    #[test]
    fn parse_pdf_fill_stroke_carries_both_colors() {
        let content = b"stream\n0 1 0 RG 1 0 1 rg 0 0 10 10 re B*\nendstream";
        let paths = parse_pdf_stream_fixture(content).unwrap();

        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].paint_mode, PdfPaintMode::FillStroke);
        assert_eq!(
            paths[0].stroke_color,
            Some(PdfRgbColor { r: 0, g: 255, b: 0 })
        );
        assert_eq!(
            paths[0].fill_color,
            Some(PdfRgbColor {
                r: 255,
                g: 0,
                b: 255
            })
        );
    }

    #[test]
    fn parse_pdf_graphics_state_restores_colors() {
        let content = b"stream\n1 0 0 RG q 0 0 1 RG 0 0 m 1 0 l S Q 0 1 m 1 1 l S\nendstream";
        let paths = parse_pdf_stream_fixture(content).unwrap();

        assert_eq!(paths.len(), 2);
        assert_eq!(
            paths[0].stroke_color,
            Some(PdfRgbColor { r: 0, g: 0, b: 255 })
        );
        assert_eq!(
            paths[1].stroke_color,
            Some(PdfRgbColor { r: 255, g: 0, b: 0 })
        );
    }

    #[test]
    fn parse_pdf_device_color_space_operators() {
        let content = b"stream\n/DeviceRGB CS .25 .5 .75 SCN 0 0 m 1 0 l S /DeviceGray cs .5 sc 0 0 1 1 re f\nendstream";
        let paths = parse_pdf_stream_fixture(content).unwrap();

        assert_eq!(
            paths[0].stroke_color,
            Some(PdfRgbColor {
                r: 64,
                g: 128,
                b: 191
            })
        );
        assert_eq!(
            paths[1].fill_color,
            Some(PdfRgbColor {
                r: 128,
                g: 128,
                b: 128
            })
        );
    }

    #[test]
    fn parse_pdf_close_and_discard_operators_terminate_paths() {
        let content = b"stream\n0 0 m 1 0 l n 0 0 m 1 0 l s 2 0 m 3 0 l b\nendstream";
        let paths = parse_pdf_stream_fixture(content).unwrap();

        assert_eq!(paths.len(), 2, "the path consumed by n must be discarded");
        assert_eq!(paths[0].paint_mode, PdfPaintMode::Stroke);
        assert!(paths[0].path.subpaths[0].closed);
        assert_eq!(
            paths[0].path.subpaths[0].commands.last(),
            Some(&PathCommand::Close)
        );
        assert_eq!(paths[1].paint_mode, PdfPaintMode::FillStroke);
        assert!(paths[1].path.subpaths[0].closed);
    }

    #[test]
    fn parse_pdf_eof_geometry_uses_unspecified_paint_fallback() {
        let content = b"stream\n1 0 0 RG 0 0 m 10 10 l\nendstream";
        let paths = parse_pdf_stream_fixture(content).unwrap();

        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].paint_mode, PdfPaintMode::Unspecified);
        assert_eq!(paths[0].stroke_color, None);
        assert_eq!(paths[0].fill_color, None);
    }

    #[test]
    fn parse_eps_postscript_operators() {
        let content = b"10 20 moveto 30 40 lineto closepath";
        let paths = parse_eps_paths(content).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].subpaths[0].commands.len(), 3); // M L Z
        assert!(paths[0].subpaths[0].closed);
    }

    #[test]
    fn parse_eps_curveto() {
        let content = b"0 0 moveto 10 20 30 40 50 60 curveto";
        let paths = parse_eps_paths(content).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].subpaths[0].commands.len(), 2);
    }

    #[test]
    fn parse_empty_pdf_returns_error() {
        let content = b"no paths here";
        let result = parse_pdf_paths_fixture(content);
        assert!(result.is_err());
    }

    #[test]
    fn parse_pdf_crlf_after_stream_keyword() {
        // Many real-world producers terminate the "stream" keyword with CRLF.
        let content =
            b"4 0 obj\r\n<< /Length 21 >>\r\nstream\r\n10 20 m 30 40 l\r\nendstream\r\nendobj\r\n";
        let paths = parse_pdf_paths_fixture(content).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].subpaths[0].commands.len(), 2);
        match paths[0].subpaths[0].commands[0] {
            PathCommand::MoveTo { x, y } => {
                assert!((x - 10.0 * 25.4 / 72.0).abs() < 1e-9);
                assert!((y - 20.0 * 25.4 / 72.0).abs() < 1e-9);
            }
            _ => panic!("expected MoveTo"),
        }
    }

    fn zlib_compress(data: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn parse_pdf_flate_decode_stream() {
        let operators = b"10 20 m 30 40 l 0 0 10 10 re h";
        let compressed = zlib_compress(operators);

        let mut pdf: Vec<u8> = Vec::new();
        pdf.extend_from_slice(b"4 0 obj\n<< /Length ");
        pdf.extend_from_slice(compressed.len().to_string().as_bytes());
        pdf.extend_from_slice(b" /Filter /FlateDecode >>\nstream\r\n");
        pdf.extend_from_slice(&compressed);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");

        let compressed_paths = parse_pdf_paths_fixture(&pdf).unwrap();

        let mut plain: Vec<u8> = Vec::new();
        plain.extend_from_slice(b"4 0 obj\n<< /Length 30 >>\nstream\n");
        plain.extend_from_slice(operators);
        plain.extend_from_slice(b"\nendstream\nendobj\n");
        let plain_paths = parse_pdf_paths_fixture(&plain).unwrap();

        assert_eq!(
            compressed_paths, plain_paths,
            "FlateDecode stream must parse to the same paths as the uncompressed equivalent"
        );
        assert_eq!(compressed_paths.len(), 1);
        assert_eq!(compressed_paths[0].subpaths.len(), 2);
    }

    #[test]
    fn parse_pdf_flate_decode_preserves_paint_color() {
        let operators = b"0 1 0 RG 0 0 m 72 0 l S";
        let compressed = zlib_compress(operators);
        let mut pdf = b"<< /Filter /FlateDecode >>\nstream\n".to_vec();
        pdf.extend_from_slice(&compressed);
        pdf.extend_from_slice(b"\nendstream\n");

        let paths = parse_pdf_stream_fixture(&pdf).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].paint_mode, PdfPaintMode::Stroke);
        assert_eq!(
            paths[0].stroke_color,
            Some(PdfRgbColor { r: 0, g: 255, b: 0 })
        );
    }

    #[test]
    fn parse_pdf_nested_dict_flate_detection() {
        // /Filter inside an object dict that also contains a nested dict.
        let operators = b"0 0 m 72 0 l";
        let compressed = zlib_compress(operators);
        let mut pdf: Vec<u8> = Vec::new();
        pdf.extend_from_slice(
            b"<< /Resources << /ProcSet [/PDF] >> /Filter /FlateDecode >>\nstream\n",
        );
        pdf.extend_from_slice(&compressed);
        pdf.extend_from_slice(b"\nendstream\n");
        let paths = parse_pdf_paths_fixture(&pdf).unwrap();
        assert_eq!(paths.len(), 1);
        match paths[0].subpaths[0].commands[1] {
            PathCommand::LineTo { x, .. } => assert!((x - 25.4).abs() < 1e-6),
            _ => panic!("expected LineTo"),
        }
    }

    #[test]
    fn pdf_points_scale_to_mm() {
        // 72 points = 1 inch = 25.4 mm.
        let content = b"stream\n0 0 m 72 0 l\nendstream";
        let paths = parse_pdf_paths_fixture(content).unwrap();
        match paths[0].subpaths[0].commands[1] {
            PathCommand::LineTo { x, .. } => {
                assert!((x - 25.4).abs() < 1e-6, "expected 25.4mm, got {x}")
            }
            _ => panic!("expected LineTo"),
        }
    }

    #[test]
    fn eps_points_scale_to_mm() {
        let content = b"0 0 moveto 72 0 lineto";
        let paths = parse_eps_paths(content).unwrap();
        match paths[0].subpaths[0].commands[1] {
            PathCommand::LineTo { x, .. } => {
                assert!((x - 25.4).abs() < 1e-6, "expected 25.4mm, got {x}")
            }
            _ => panic!("expected LineTo"),
        }
    }
}

#[cfg(test)]
mod document_regressions {
    use super::*;
    fn assert_pdf_coord(mm: f64, points: f64) {
        assert!(
            (mm / PT_TO_MM - points).abs() < 1e-4,
            "{mm} mm != {points} pt"
        );
    }
    use lopdf::{Document, Object, Stream, dictionary};

    fn document_with_graphics_state(
        content: &[u8],
        state: lopdf::Dictionary,
        inherited: bool,
        indirect: bool,
    ) -> Vec<u8> {
        let mut doc = Document::load_mem(&document(content)).unwrap();
        let page_id = *doc.get_pages().values().next().unwrap();
        let owner = if inherited {
            doc.get_dictionary(page_id)
                .unwrap()
                .get(b"Parent")
                .unwrap()
                .as_reference()
                .unwrap()
        } else {
            page_id
        };
        let state = if indirect {
            Object::Reference(doc.add_object(state))
        } else {
            Object::Dictionary(state)
        };
        let states = dictionary! {"GS0" => state};
        let states = if indirect {
            Object::Reference(doc.add_object(states))
        } else {
            Object::Dictionary(states)
        };
        let resources = dictionary! {"ExtGState" => states};
        let resources = if indirect {
            Object::Reference(doc.add_object(resources))
        } else {
            Object::Dictionary(resources)
        };
        doc.get_dictionary_mut(owner)
            .unwrap()
            .set("Resources", resources);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn accepts_opaque_graphics_states_without_changing_geometry_or_colors() {
        let drawing = b"1 0 0 RG 0 0 m 72 0 l S q 2 0 0 3 10 20 cm 0 0 1 rg 0 0 10 20 re f Q 0 10 m 72 10 l S";
        let with_state = [b"/GS0 gs ".as_slice(), drawing].concat();
        let expected = parse_pdf_painted_paths(&document(drawing)).unwrap();
        // Common exporter defaults, including the line styling already accepted
        // as standalone PDF operators by this centerline path importer.
        let state = dictionary! {
            "Type" => "ExtGState", "CA" => 1, "ca" => 1.0,
            "BM" => "Normal", "SMask" => "None", "AIS" => false, "TK" => true,
            "LW" => 0.5, "LC" => 1, "LJ" => 2, "ML" => 10,
            "D" => vec![Object::Array(vec![]), 0.into()],
            "OP" => false, "op" => false, "OPM" => 1,
            "RI" => "RelativeColorimetric", "SA" => true, "FL" => 1,
            "SM" => 0.02, "TR" => "Identity", "TR2" => "Default"
        };
        for inherited in [false, true] {
            for indirect in [false, true] {
                let pdf =
                    document_with_graphics_state(&with_state, state.clone(), inherited, indirect);
                assert_eq!(parse_pdf_painted_paths(&pdf).unwrap(), expected);
            }
        }
    }

    #[test]
    fn accepts_empty_and_normal_blend_array_graphics_states() {
        for state in [
            dictionary! {},
            dictionary! {"BM" => vec![Object::Name(b"Normal".to_vec())]},
        ] {
            let pdf = document_with_graphics_state(b"/GS0 gs 0 0 72 72 re f", state, false, false);
            let paths = parse_pdf_painted_paths(&pdf).unwrap();
            assert_eq!(paths.len(), 1);
            assert_eq!(paths[0].paint_mode, PdfPaintMode::Fill);
        }
    }

    #[test]
    fn rejects_graphics_state_effects_instead_of_importing_partial_artwork() {
        for (key, value) in [
            ("ca", Object::Real(0.5)),
            ("CA", 0.into()),
            ("BM", "Multiply".into()),
            ("SMask", dictionary! {"S" => "Alpha"}.into()),
            ("OP", true.into()),
            ("op", true.into()),
            (
                "D",
                vec![Object::Array(vec![2.into(), 3.into()]), 0.into()].into(),
            ),
            ("TR", dictionary! {"FunctionType" => 2}.into()),
            ("UnknownEffect", true.into()),
        ] {
            let mut state = dictionary! {};
            state.set(key, value);
            let pdf = document_with_graphics_state(
                b"0 0 m 10 10 l S /GS0 gs 20 20 30 30 re f",
                state,
                false,
                true,
            );
            let error = parse_pdf_painted_paths(&pdf).unwrap_err();
            assert!(error.contains(key), "{key}: {error}");
            assert!(error.contains("flatten"), "{key}: {error}");
        }
    }

    #[test]
    fn rejects_missing_or_malformed_graphics_state_resources() {
        for content in [
            b"/Missing gs 0 0 10 10 re f".as_slice(),
            b"gs 0 0 10 10 re f",
            b"12 gs 0 0 10 10 re f",
        ] {
            let pdf = document_with_graphics_state(content, dictionary! {}, false, false);
            assert!(parse_pdf_painted_paths(&pdf).is_err());
        }
        for state in [
            dictionary! {"ca" => "1"},
            dictionary! {"CA" => 2},
            dictionary! {"LW" => -1},
        ] {
            let pdf = document_with_graphics_state(b"/GS0 gs 0 0 10 10 re f", state, false, false);
            assert!(parse_pdf_painted_paths(&pdf).is_err());
        }
    }

    #[test]
    fn unused_graphics_state_effects_do_not_block_import() {
        let drawing = b"0 0 10 10 re f";
        let pdf = document_with_graphics_state(drawing, dictionary! {"ca" => 0.5}, false, false);
        assert_eq!(
            parse_pdf_painted_paths(&pdf).unwrap(),
            parse_pdf_painted_paths(&document(drawing)).unwrap()
        );
    }

    #[test]
    fn page_resources_replace_inherited_graphics_states() {
        let bytes = document_with_graphics_state(
            b"/GS0 gs 0 0 10 10 re f",
            dictionary! {"ca" => 0.5},
            true,
            true,
        );
        let mut doc = Document::load_mem(&bytes).unwrap();
        let page_id = *doc.get_pages().values().next().unwrap();
        let alpha = doc.add_object(Object::Real(1.0));
        doc.get_dictionary_mut(page_id).unwrap().set(
            "Resources",
            dictionary! {
                "ExtGState" => dictionary! {"GS0" => dictionary! {"ca" => alpha}}
            },
        );
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        assert_eq!(parse_pdf_painted_paths(&bytes).unwrap().len(), 1);

        // A page resource dictionary shadows the whole inherited dictionary;
        // missing names must not fall back to the parent's state.
        doc.get_dictionary_mut(page_id)
            .unwrap()
            .set("Resources", dictionary! {});
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        assert!(parse_pdf_painted_paths(&bytes).is_err());
    }

    fn document(content: &[u8]) -> Vec<u8> {
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.to_vec()));
        // This form is deliberately unreferenced by the page.
        doc.add_object(Stream::new(dictionary!{"Type"=>"XObject", "Subtype"=>"Form", "BBox"=>vec![0.into(),0.into(),100.into(),100.into()]}, b"0 0 100 100 re f".to_vec()));
        let page_id = doc.add_object(dictionary!{"Type"=>"Page", "Parent"=>pages_id,"MediaBox"=>vec![0.into(),0.into(),100.into(),100.into()],"Contents"=>content_id});
        doc.objects.insert(
            pages_id,
            Object::Dictionary(
                dictionary! {"Type"=>"Pages", "Kids"=>vec![page_id.into()],"Count"=>1},
            ),
        );
        let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
        doc.trailer.set("Root", root);
        let mut bytes = Vec::new();
        doc.compress();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }
    #[test]
    fn encoding_names_in_metadata_comments_and_stream_bytes_are_not_encodings() {
        for title in [
            "Guide to /Encrypt settings",
            "Guide to /ObjStm settings",
            "Nested (title /Encr#79pt) and \"escapes\"",
        ] {
            let mut doc =
                Document::load_mem(&document(b"% /Encrypt /ObjStm\n0 0 m 10 10 l S")).unwrap();
            let info = doc.add_object(dictionary! {"Title" => Object::string_literal(title)});
            doc.trailer.set("Info", info);
            doc.add_object(Stream::new(
                dictionary! {},
                b"binary\0/Encrypt /ObjStm /Encr#79pt".to_vec(),
            ));
            let mut bytes = Vec::new();
            doc.save_to(&mut bytes).unwrap();
            bytes.extend_from_slice(b"\n% /Encrypt /ObjStm\n");
            assert_eq!(parse_pdf_painted_paths(&bytes).unwrap().len(), 1);
        }
    }

    #[test]
    fn ordinary_solid_dash_reset_is_geometry_neutral() {
        let drawing = b"0 0 m 72 0 l 72 36 l S";
        assert_eq!(
            parse_pdf_painted_paths(&document(&[b"[] 0 d ".as_slice(), drawing].concat())).unwrap(),
            parse_pdf_painted_paths(&document(drawing)).unwrap(),
        );
        for pattern in ["[2 3] 0 d", "[] -1 d", "[] /Bad d", "0 d"] {
            assert!(
                parse_pdf_painted_paths(&document(format!("{pattern} 0 0 10 10 re S").as_bytes()))
                    .is_err()
            );
        }
    }

    #[test]
    fn compressed_and_split_page_streams_preserve_state_and_geometry() {
        let parts: [&[u8]; 3] = [
            b"1 0 0 RG q 2 0 0 3 10 20 cm",
            b"0 0 m 10 10 l S Q",
            b"0 0 1 rg 20 20 10 10 re f",
        ];
        let expected = parse_pdf_painted_paths(&document(&parts.join(b" ".as_slice()))).unwrap();
        for compress in [false, true] {
            let mut doc = Document::load_mem(&document(b"")).unwrap();
            let ids: Vec<Object> = parts
                .iter()
                .map(|part| {
                    let mut stream = Stream::new(dictionary! {}, part.to_vec());
                    if compress {
                        stream.compress().unwrap();
                    }
                    doc.add_object(stream).into()
                })
                .collect();
            let page = *doc.get_pages().values().next().unwrap();
            doc.get_dictionary_mut(page).unwrap().set("Contents", ids);
            let mut bytes = Vec::new();
            doc.save_to(&mut bytes).unwrap();
            assert_eq!(parse_pdf_painted_paths(&bytes).unwrap(), expected);
        }
    }

    #[test]
    fn compressed_object_streams_match_classic_encoding() {
        let expected = parse_pdf_painted_paths(&document(b"0 0 m 72 0 l 72 36 l S")).unwrap();
        let mut doc = Document::load_mem(&document(b"0 0 m 72 0 l 72 36 l S")).unwrap();
        let mut modern = Vec::new();
        doc.save_modern(&mut modern).unwrap();
        assert_eq!(parse_pdf_painted_paths(&modern).unwrap(), expected);
        // The writer can represent identical objects without compressed xrefs.
        // This is a fixture conversion, not an unbounded production fallback.
        let mut expanded = Document::load_mem(&modern).unwrap();
        let mut classic = Vec::new();
        expanded.save_to(&mut classic).unwrap();
        assert_eq!(parse_pdf_painted_paths(&classic).unwrap(), expected);
    }

    #[test]
    fn referenced_forms_and_images_preserve_artwork() {
        for subtype in ["Form", "Image"] {
            let mut doc = Document::load_mem(&document(b"0 0 m 10 10 l S /X0 Do")).unwrap();
            let xobject = if subtype == "Form" {
                Stream::new(
                    dictionary! {
                        "Type" => "XObject", "Subtype" => "Form",
                        "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
                        "Matrix" => vec![2.into(), 0.into(), 0.into(), 2.into(), 10.into(), 20.into()],
                        "Resources" => dictionary! {},
                    },
                    b"0 0 10 10 re f".to_vec(),
                )
            } else {
                Stream::new(
                    dictionary! {
                        "Type" => "XObject", "Subtype" => "Image",
                        "Width" => 1, "Height" => 1,
                        "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
                    },
                    vec![0],
                )
            };
            let object_id = doc.add_object(xobject);
            let page_id = *doc.get_pages().values().next().unwrap();
            doc.get_dictionary_mut(page_id).unwrap().set(
                "Resources",
                dictionary! {
                    "XObject" => dictionary! {"X0" => object_id},
                },
            );
            let mut bytes = Vec::new();
            doc.save_to(&mut bytes).unwrap();
            let artwork = parse_pdf_artwork(&bytes).unwrap();
            if subtype == "Image" {
                assert_eq!(artwork.paths.len(), 1);
                assert_eq!(artwork.images.len(), 1);
                let image = image::load_from_memory(&artwork.images[0].png)
                    .unwrap()
                    .to_rgba8();
                assert_eq!(image.get_pixel(0, 0).0, [0, 0, 0, 255]);
                assert!(
                    parse_pdf_painted_paths(&bytes)
                        .unwrap_err()
                        .contains("embedded images")
                );
            } else {
                assert_eq!(artwork.paths.len(), 2);
                let bounds = artwork.paths[1].path.visual_bounds().unwrap();
                assert_pdf_coord(bounds.min.x, 10.0);
                assert_pdf_coord(bounds.min.y, 20.0);
                assert_pdf_coord(bounds.max.x, 30.0);
                assert_pdf_coord(bounds.max.y, 40.0);
            }
        }
    }

    #[test]
    fn eager_cross_reference_decompression_stays_bounded() {
        use std::io::Write;
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder
            .write_all(&vec![0; PDF_EAGER_STREAM_LIMIT + 1])
            .unwrap();
        let compressed = encoder.finish().unwrap();
        let mut bytes = b"%PDF-1.5\n".to_vec();
        let offset = bytes.len();
        bytes.extend_from_slice(format!(
            "1 0 obj\n<< /Type /XRef /Size 1 /W [1 4 2] /Filter /FlateDecode /Length {} >>\nstream\n",
            compressed.len(),
        ).as_bytes());
        bytes.extend_from_slice(&compressed);
        bytes.extend_from_slice(
            format!("\nendstream\nendobj\nstartxref\n{offset}\n%%EOF\n").as_bytes(),
        );
        let error = parse_pdf_painted_paths(&bytes).unwrap_err();
        assert!(error.contains("limit"), "{error}");
    }

    #[test]
    fn eager_cross_reference_predictor_rows_are_bounded_before_allocation() {
        use std::io::Write;
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&[0, 0]).unwrap();
        let compressed = encoder.finish().unwrap();
        let mut bytes = b"%PDF-1.5\n".to_vec();
        let offset = bytes.len();
        bytes.extend_from_slice(format!("1 0 obj\n<< /Type /XRef /Size 1 /W [1 4 2] /Filter /FlateDecode /DecodeParms << /Predictor 12 /Columns 1000000000 /Colors 4 /BitsPerComponent 8 >> /Length {} >>\nstream\n",compressed.len()).as_bytes());
        bytes.extend_from_slice(&compressed);
        bytes.extend_from_slice(
            format!("\nendstream\nendobj\nstartxref\n{offset}\n%%EOF\n").as_bytes(),
        );
        assert!(parse_pdf_artwork(&bytes).unwrap_err().contains("limit"));
    }

    #[test]
    fn real_encryption_is_rejected_even_with_an_empty_password() {
        for password in ["", "secret"] {
            let mut doc = Document::load_mem(&document(b"0 0 m 10 10 l S")).unwrap();
            doc.trailer.set(
                "ID",
                vec![
                    Object::string_literal("fixed-test-file-id"),
                    Object::string_literal("fixed-test-file-id"),
                ],
            );
            let state = lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V2 {
                document: &doc,
                owner_password: "owner",
                user_password: password,
                key_length: 128,
                permissions: lopdf::Permissions::all(),
            })
            .unwrap();
            doc.encrypt(&state).unwrap();
            let mut bytes = Vec::new();
            doc.save_to(&mut bytes).unwrap();
            for escape_name in [false, true] {
                let mut encoded = bytes.clone();
                if escape_name {
                    // Trailer follows all objects, so this does not move any
                    // offsets recorded in the cross-reference table.
                    let offset = find_bytes(&encoded, b"/Encrypt", 0).unwrap();
                    encoded.splice(offset..offset + 8, b"/Encr#79pt".iter().copied());
                }
                assert!(
                    parse_pdf_painted_paths(&encoded)
                        .unwrap_err()
                        .contains("Encrypted PDFs")
                );
            }
        }
    }

    #[test]
    fn unused_form_does_not_become_geometry() {
        let paths = parse_pdf_painted_paths(&document(b"0 0 m 10 10 l S")).unwrap();
        assert_eq!(paths.len(), 1);
    }
    #[test]
    fn clips_visible_paths_and_rejects_missing_forms() {
        let paths =
            parse_pdf_painted_paths(&document(b"0 0 10 10 re W n 0 0 m 20 20 l S")).unwrap();
        let bounds = paths[0].path.visual_bounds().unwrap();
        assert_pdf_coord(bounds.max.x, 10.0);
        assert_pdf_coord(bounds.max.y, 10.0);
        assert!(
            parse_pdf_painted_paths(&document(b"0 0 m 10 10 l S /Fm0 Do"))
                .unwrap_err()
                .contains("resources")
        );
    }
    #[test]
    fn rejects_oversized_page_content() {
        let content = vec![b' '; PDF_CONTENT_LIMIT + 1];
        assert!(parse_pdf_painted_paths(&document(&content)).is_err());
    }
    fn with_xobject(content: &[u8], xobject: Stream) -> Vec<u8> {
        let mut doc = Document::load_mem(&document(content)).unwrap();
        let id = doc.add_object(xobject);
        let page = *doc.get_pages().values().next().unwrap();
        doc.get_dictionary_mut(page).unwrap().set(
            "Resources",
            dictionary! {"XObject" => dictionary! {"X" => id}},
        );
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    fn gray_image(width: i64, height: i64, bits: i64, bytes: Vec<u8>) -> Stream {
        Stream::new(
            dictionary! {"Type"=>"XObject", "Subtype"=>"Image", "Width"=>width, "Height"=>height,
            "ColorSpace"=>"DeviceGray", "BitsPerComponent"=>bits},
            bytes,
        )
    }

    #[test]
    fn images_preserve_top_row_decode_and_clipping() {
        let mut image = gray_image(4, 2, 1, vec![0b11100000, 0b10000000]);
        image.dict.set("Decode", vec![1.into(), 0.into()]);
        let bytes = with_xobject(b"q 10 10 20 20 re W n 40 0 0 20 10 10 cm /X Do Q", image);
        let artwork = parse_pdf_artwork(&bytes).unwrap();
        let source = &artwork.images[0];
        let pixels = image::load_from_memory(&source.png).unwrap().to_rgba8();
        assert_eq!(pixels.get_pixel(0, 0).0, [0, 0, 0, 255]);
        assert_eq!(pixels.get_pixel(1, 0).0, [0, 0, 0, 255]);
        assert_eq!(pixels.get_pixel(1, 1).0, [255, 255, 255, 255]);
        assert_eq!(pixels.get_pixel(2, 0).0, [0, 0, 0, 255]);
        let clip_bounds = source.clip.as_ref().unwrap().visual_bounds().unwrap();
        assert_pdf_coord(clip_bounds.min.x, 10.0);
        assert_pdf_coord(clip_bounds.max.x, 30.0);
        let top = source.transform.apply(&Point2D::new(0.0, 0.0));
        let bottom = source.transform.apply(&Point2D::new(0.0, 1.0));
        assert_pdf_coord(top.y, 30.0);
        assert_pdf_coord(bottom.y, 10.0);
        assert_pdf_coord(artwork.page_to_canvas.apply(&top).y, 70.0);
    }

    #[test]
    fn jpeg_and_png_predictor_images_decode() {
        let image = image::GrayImage::from_raw(2, 2, vec![0, 80, 160, 255]).unwrap();
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 100)
            .encode_image(&image)
            .unwrap();
        let mut stream = gray_image(2, 2, 8, jpeg);
        stream.dict.set("Filter", "DCTDecode");
        let artwork = parse_pdf_artwork(&with_xobject(b"10 0 0 10 5 5 cm /X Do", stream)).unwrap();
        let decoded = image::load_from_memory(&artwork.images[0].png)
            .unwrap()
            .to_luma8();
        for (a, b) in decoded.as_raw().iter().zip(image.as_raw()) {
            assert!(a.abs_diff(*b) < 3);
        }
        let mut stream = gray_image(2, 2, 8, vec![0, 0, 80, 2, 160, 175]);
        {
            use std::io::Write;
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(&stream.content).unwrap();
            stream.set_content(encoder.finish().unwrap());
            stream.dict.set("Filter", "FlateDecode");
        }
        stream.dict.set(
            "DecodeParms",
            dictionary! {"Predictor"=>12,"Columns"=>2,"Colors"=>1,"BitsPerComponent"=>8},
        );
        let artwork = parse_pdf_artwork(&with_xobject(b"10 0 0 10 5 5 cm /X Do", stream)).unwrap();
        assert_eq!(
            image::load_from_memory(&artwork.images[0].png)
                .unwrap()
                .to_luma8(),
            image
        );
    }

    #[test]
    fn unsupported_images_fail_without_returning_partial_paths() {
        for (key, value) in [
            ("SMask", Object::Name(b"None".to_vec())),
            ("ImageMask", true.into()),
            ("ColorSpace", "Indexed".into()),
            (
                "DecodeParms",
                Object::Dictionary(dictionary! {"Predictor"=>2}),
            ),
            ("Width", 100_000_000.into()),
        ] {
            let mut image = gray_image(1, 1, 8, vec![0]);
            image.dict.set(key, value);
            assert!(
                parse_pdf_artwork(&with_xobject(b"0 0 m 10 10 l S /X Do", image)).is_err(),
                "{key}"
            );
        }
    }

    #[test]
    fn form_cycles_groups_and_missing_resources_fail_atomically() {
        let form = Stream::new(
            dictionary! {"Type"=>"XObject", "Subtype"=>"Form", "BBox"=>vec![0.into(),0.into(),100.into(),100.into()]},
            b"/X Do".to_vec(),
        );
        assert!(
            parse_pdf_artwork(&with_xobject(b"0 0 m 10 10 l S /X Do", form.clone()))
                .unwrap_err()
                .contains("cyclic")
        );
        let mut grouped = form.clone();
        grouped.dict.set("Group", dictionary! {"S"=>"Transparency"});
        assert!(
            parse_pdf_artwork(&with_xobject(b"/X Do", grouped))
                .unwrap_err()
                .contains("compositing")
        );
        let mut scoped = form;
        scoped.dict.set("Resources", dictionary! {});
        assert!(parse_pdf_artwork(&with_xobject(b"/X Do", scoped)).is_err());
    }

    #[test]
    fn repeated_forms_share_the_content_budget() {
        let mut payload = b"0 0 m 1 1 l S ".to_vec();
        payload.resize(1024 * 1024, b' ');
        let mut form = Stream::new(
            dictionary! {"Type"=>"XObject", "Subtype"=>"Form", "BBox"=>vec![0.into(),0.into(),100.into(),100.into()]},
            payload,
        );
        form.compress().unwrap();
        let error =
            parse_pdf_artwork(&with_xobject(b"/X Do ".repeat(33).as_slice(), form)).unwrap_err();
        assert!(error.contains("limit"), "{error}");
    }

    #[test]
    fn repeated_images_share_the_pixel_budget() {
        let mut image = gray_image(1000, 1000, 1, vec![0; 125_000]);
        image.compress().unwrap();
        let error =
            parse_pdf_artwork(&with_xobject(b"/X Do ".repeat(17).as_slice(), image)).unwrap_err();
        assert!(error.contains("image pixel limit"), "{error}");
    }

    #[test]
    fn clipped_curves_and_graphics_state_restore_preserve_visible_bounds() {
        let bytes = document(b"q 10 10 10 10 re W n 0 0 m 0 30 30 30 30 0 c S Q 60 60 10 10 re f");
        let artwork = parse_pdf_artwork(&bytes).unwrap();
        // The curve rises above the clipping rectangle and never crosses it;
        // only the subsequent, unclipped rectangle remains.
        assert_eq!(artwork.paths.len(), 1);
        let bounds = artwork.paths[0].path.visual_bounds().unwrap();
        assert_pdf_coord(bounds.min.x, 60.0);
        assert_pdf_coord(bounds.min.y, 60.0);
    }
    #[test]
    fn content_operation_limit_and_malformed_tail_do_not_return_partial_artwork() {
        let mut content = b"0 0 m 10 10 l S ".to_vec();
        content.extend_from_slice(&b"q Q ".repeat(100_000));
        assert!(
            parse_pdf_artwork(&document(&content))
                .unwrap_err()
                .contains("operation limit")
        );
        assert!(
            parse_pdf_artwork(&document(b"0 0 m 10 10 l S [ 1"))
                .unwrap_err()
                .contains("malformed")
        );
    }
}

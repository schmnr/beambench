//! Simplified PDF/AI/EPS path extractor (no external dependencies).

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
fn flate_decode(data: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(data)
        .read_to_end(&mut out)
        .map_err(|e| format!("FlateDecode error: {e}"))?;
    Ok(out)
}

/// Extract path operators from PDF content.
///
/// Operates on raw bytes: finds `stream`/`endstream` pairs (accepting both
/// `\n` and `\r\n` after the `stream` keyword), inflates FlateDecode-compressed
/// streams, and feeds each decoded content stream to the operator parser.
/// No xref/object-graph parsing — encrypted PDFs and exotic filters
/// (e.g. LZW, predictors via /DecodeParms) are not supported.
pub fn parse_pdf_painted_paths(content: &[u8]) -> Result<Vec<PdfPaintedPath>, String> {
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
        let paths = parse_pdf_paths(content).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].subpaths[0].commands.len(), 2);
    }

    #[test]
    fn parse_pdf_rectangle() {
        let content = b"stream\n10 20 50 30 re\nendstream";
        let paths = parse_pdf_paths(content).unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].subpaths[0].closed);
        assert_eq!(paths[0].subpaths[0].commands.len(), 5); // M L L L Z
    }

    #[test]
    fn parse_pdf_curveto() {
        let content = b"stream\n0 0 m 10 20 30 40 50 60 c\nendstream";
        let paths = parse_pdf_paths(content).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].subpaths[0].commands.len(), 2); // M C
    }

    #[test]
    fn parse_pdf_curve_shorthands_preserve_implicit_controls() {
        let content = b"stream\n1 2 m 3 4 5 6 v 7 8 9 10 y S\nendstream";
        let paths = parse_pdf_paths(content).unwrap();
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
        let paths = parse_pdf_paths(content).unwrap();
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
        let paths = parse_pdf_paths(content).unwrap();

        assert_eq!(paths.len(), 2);
        assert_command_endpoint(&paths[0].subpaths[0].commands[0], 12.0, 23.0);
        assert_command_endpoint(&paths[0].subpaths[0].commands[1], 14.0, 26.0);
        assert_command_endpoint(&paths[1].subpaths[0].commands[0], 11.0, 21.0);
        assert_command_endpoint(&paths[1].subpaths[0].commands[1], 12.0, 22.0);
    }

    #[test]
    fn parse_pdf_ctm_transforms_rectangle_corners() {
        let content = b"stream\n0 1 -1 0 100 200 cm 10 20 30 40 re f\nendstream";
        let paths = parse_pdf_paths(content).unwrap();
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
        let paths = parse_pdf_painted_paths(content).unwrap();

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
        let paths = parse_pdf_painted_paths(content).unwrap();

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
        let paths = parse_pdf_painted_paths(content).unwrap();

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
        let paths = parse_pdf_painted_paths(content).unwrap();

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
        let paths = parse_pdf_painted_paths(content).unwrap();

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
        let paths = parse_pdf_painted_paths(content).unwrap();

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
        let paths = parse_pdf_painted_paths(content).unwrap();

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
        let result = parse_pdf_paths(content);
        assert!(result.is_err());
    }

    #[test]
    fn parse_pdf_crlf_after_stream_keyword() {
        // Many real-world producers terminate the "stream" keyword with CRLF.
        let content =
            b"4 0 obj\r\n<< /Length 21 >>\r\nstream\r\n10 20 m 30 40 l\r\nendstream\r\nendobj\r\n";
        let paths = parse_pdf_paths(content).unwrap();
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

        let compressed_paths = parse_pdf_paths(&pdf).unwrap();

        let mut plain: Vec<u8> = Vec::new();
        plain.extend_from_slice(b"4 0 obj\n<< /Length 30 >>\nstream\n");
        plain.extend_from_slice(operators);
        plain.extend_from_slice(b"\nendstream\nendobj\n");
        let plain_paths = parse_pdf_paths(&plain).unwrap();

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

        let paths = parse_pdf_painted_paths(&pdf).unwrap();
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
        let paths = parse_pdf_paths(&pdf).unwrap();
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
        let paths = parse_pdf_paths(content).unwrap();
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

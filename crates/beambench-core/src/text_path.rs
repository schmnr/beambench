use beambench_common::Point2D;
use beambench_common::path::{PathCommand, SubPath, VecPath};

use crate::object::{TextAlignment, TextAlignmentV, TextCirclePlacement, TextFontSource};
use crate::vector::boolean::weld_shapes;
use crate::vector::flatten::flatten_vecpath;
use crate::vector::text_to_path::resolve_text_in_box_with_options;

/// Guide paths are flattened at 5× finer resolution than general-purpose
/// tolerance to preserve curvature for smooth text-on-path layout.
const GUIDE_FLATTEN_TOLERANCE_MM: f64 = 0.01;

/// Result of laying text along a path, including font resolution metadata.
pub(crate) struct PathTextResult {
    pub path: VecPath,
    pub resolved_font_source: TextFontSource,
    pub resolved_font_key: String,
    pub missing_font: bool,
    pub missing_glyphs: Vec<String>,
}

#[derive(Clone, Copy)]
struct SegmentFrame {
    start: Point2D,
    end: Point2D,
    start_len: f64,
    len: f64,
}

fn path_frames(path: &VecPath) -> Vec<SegmentFrame> {
    let mut frames = Vec::new();
    let mut cursor = 0.0;

    for polyline in flatten_vecpath(path, GUIDE_FLATTEN_TOLERANCE_MM) {
        if polyline.points.len() < 2 {
            continue;
        }

        let mut push_segment = |start: Point2D, end: Point2D, cursor: &mut f64| {
            let len = start.distance_to(&end);
            if len <= f64::EPSILON {
                return;
            }
            frames.push(SegmentFrame {
                start,
                end,
                start_len: *cursor,
                len,
            });
            *cursor += len;
        };

        for window in polyline.points.windows(2) {
            push_segment(window[0], window[1], &mut cursor);
        }

        if polyline.closed && !polyline.points.is_empty() {
            push_segment(
                *polyline.points.last().unwrap(),
                polyline.points[0],
                &mut cursor,
            );
        }
    }

    frames
}

fn total_length(frames: &[SegmentFrame]) -> f64 {
    frames
        .last()
        .map(|seg| seg.start_len + seg.len)
        .unwrap_or(0.0)
}

fn guide_is_closed(path: &VecPath) -> bool {
    flatten_vecpath(path, GUIDE_FLATTEN_TOLERANCE_MM)
        .iter()
        .any(|poly| poly.closed)
}

/// Unit tangent vector from a segment's start to end.
fn segment_tangent(seg: &SegmentFrame) -> Point2D {
    let inv = 1.0 / seg.len;
    Point2D::new(
        (seg.end.x - seg.start.x) * inv,
        (seg.end.y - seg.start.y) * inv,
    )
}

/// Normalized linear blend of two unit tangent vectors.
fn lerp_tangent(a: &Point2D, b: &Point2D, t: f64) -> Point2D {
    let x = a.x * (1.0 - t) + b.x * t;
    let y = a.y * (1.0 - t) + b.y * t;
    let len = (x * x + y * y).sqrt();
    if len < f64::EPSILON {
        *a
    } else {
        Point2D::new(x / len, y / len)
    }
}

/// Hermite smoothstep: t² × (3 − 2t)
fn smoothstep(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// Angle-threshold for normal smoothing: only blend adjacent segment tangents
/// when their dot product exceeds this (angle < ~11°). This catches curve-
/// flattening artifacts while preserving intentional corners.
const SMOOTH_DOT_THRESHOLD: f64 = 0.98;

fn sample_frame(
    frames: &[SegmentFrame],
    distance: f64,
    closed: bool,
) -> Option<(Point2D, Point2D, Point2D)> {
    let total = total_length(frames);
    if total <= f64::EPSILON || frames.is_empty() {
        return None;
    }

    let target = if closed {
        distance.rem_euclid(total)
    } else {
        distance.clamp(0.0, total)
    };

    let seg_idx = frames
        .iter()
        .position(|seg| target <= seg.start_len + seg.len)
        .unwrap_or(frames.len() - 1);
    let segment = &frames[seg_idx];

    let local_t = if segment.len <= f64::EPSILON {
        0.0
    } else {
        ((target - segment.start_len) / segment.len).clamp(0.0, 1.0)
    };
    let origin = segment.start.lerp(&segment.end, local_t);

    // Compute raw tangent for this segment
    let raw_tangent = segment_tangent(segment);

    // Angle-aware smoothing near segment boundaries
    let tangent = if local_t < 0.3 && seg_idx > 0 {
        let prev_tangent = segment_tangent(&frames[seg_idx - 1]);
        let dot = raw_tangent.x * prev_tangent.x + raw_tangent.y * prev_tangent.y;
        if dot > SMOOTH_DOT_THRESHOLD {
            // Blend: at local_t=0 fully blended (50/50), at local_t=0.3 fully raw
            let blend_t = smoothstep(local_t / 0.3);
            lerp_tangent(&prev_tangent, &raw_tangent, 0.5 + 0.5 * blend_t)
        } else {
            raw_tangent
        }
    } else if local_t > 0.7 && seg_idx + 1 < frames.len() {
        let next_tangent = segment_tangent(&frames[seg_idx + 1]);
        let dot = raw_tangent.x * next_tangent.x + raw_tangent.y * next_tangent.y;
        if dot > SMOOTH_DOT_THRESHOLD {
            let blend_t = smoothstep((local_t - 0.7) / 0.3);
            lerp_tangent(&raw_tangent, &next_tangent, 0.5 * blend_t)
        } else {
            raw_tangent
        }
    } else {
        raw_tangent
    };

    let normal = Point2D::new(-tangent.y, tangent.x);
    Some((origin, tangent, normal))
}

/// Compute the x-coordinate range of a subpath (min_x, max_x).
fn subpath_x_range(subpath: &SubPath) -> (f64, f64) {
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    for cmd in &subpath.commands {
        match *cmd {
            PathCommand::MoveTo { x, .. } | PathCommand::LineTo { x, .. } => {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
            }
            PathCommand::QuadTo { cx, x, .. } => {
                min_x = min_x.min(cx).min(x);
                max_x = max_x.max(cx).max(x);
            }
            PathCommand::CubicTo { c1x, c2x, x, .. } => {
                min_x = min_x.min(c1x).min(c2x).min(x);
                max_x = max_x.max(c1x).max(c2x).max(x);
            }
            PathCommand::Close => {}
        }
    }
    (min_x, max_x)
}

/// Apply rigid-body transform: rotate glyph around its center x-coordinate
/// to align with the guide's tangent/normal frame at that position.
fn rigid_transform(
    x: f64,
    y: f64,
    center_x: f64,
    origin: &Point2D,
    tangent: &Point2D,
    normal: &Point2D,
) -> Point2D {
    let dx = x - center_x;
    Point2D::new(
        origin.x + tangent.x * dx + normal.x * y,
        origin.y + tangent.y * dx + normal.y * y,
    )
}

/// Transform a subpath using a pre-computed rigid frame (origin, tangent, normal)
/// at the given center_x. All points are mapped relative to center_x.
fn rigid_transform_subpath(
    subpath: &SubPath,
    center_x: f64,
    origin: &Point2D,
    tangent: &Point2D,
    normal: &Point2D,
) -> SubPath {
    let mut mapped = SubPath::new();
    mapped.closed = subpath.closed;
    for cmd in &subpath.commands {
        let next = match *cmd {
            PathCommand::MoveTo { x, y } => {
                let p = rigid_transform(x, y, center_x, origin, tangent, normal);
                PathCommand::MoveTo { x: p.x, y: p.y }
            }
            PathCommand::LineTo { x, y } => {
                let p = rigid_transform(x, y, center_x, origin, tangent, normal);
                PathCommand::LineTo { x: p.x, y: p.y }
            }
            PathCommand::QuadTo { cx, cy, x, y } => {
                let c = rigid_transform(cx, cy, center_x, origin, tangent, normal);
                let p = rigid_transform(x, y, center_x, origin, tangent, normal);
                PathCommand::QuadTo {
                    cx: c.x,
                    cy: c.y,
                    x: p.x,
                    y: p.y,
                }
            }
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                let c1 = rigid_transform(c1x, c1y, center_x, origin, tangent, normal);
                let c2 = rigid_transform(c2x, c2y, center_x, origin, tangent, normal);
                let p = rigid_transform(x, y, center_x, origin, tangent, normal);
                PathCommand::CubicTo {
                    c1x: c1.x,
                    c1y: c1.y,
                    c2x: c2.x,
                    c2y: c2.y,
                    x: p.x,
                    y: p.y,
                }
            }
            PathCommand::Close => PathCommand::Close,
        };
        mapped.commands.push(next);
    }
    mapped
}

/// Map a single point using per-point distortion.
/// x maps to guide distance (scaled), y offsets along the local normal.
fn distort_point(
    x: f64,
    y: f64,
    distance_scale: f64,
    frames: &[SegmentFrame],
    closed: bool,
) -> Point2D {
    let guide_distance = x * distance_scale;
    if let Some((origin, _tangent, normal)) = sample_frame(frames, guide_distance, closed) {
        Point2D::new(origin.x + normal.x * y, origin.y + normal.y * y)
    } else {
        Point2D::new(x, y)
    }
}

/// Transform a subpath using per-point distortion: each control point
/// gets its own frame sample along the guide path.
fn distort_transform_subpath(
    subpath: &SubPath,
    distance_scale: f64,
    frames: &[SegmentFrame],
    closed: bool,
) -> SubPath {
    let mut mapped = SubPath::new();
    mapped.closed = subpath.closed;
    for cmd in &subpath.commands {
        let next = match *cmd {
            PathCommand::MoveTo { x, y } => {
                let p = distort_point(x, y, distance_scale, frames, closed);
                PathCommand::MoveTo { x: p.x, y: p.y }
            }
            PathCommand::LineTo { x, y } => {
                let p = distort_point(x, y, distance_scale, frames, closed);
                PathCommand::LineTo { x: p.x, y: p.y }
            }
            PathCommand::QuadTo { cx, cy, x, y } => {
                let c = distort_point(cx, cy, distance_scale, frames, closed);
                let p = distort_point(x, y, distance_scale, frames, closed);
                PathCommand::QuadTo {
                    cx: c.x,
                    cy: c.y,
                    x: p.x,
                    y: p.y,
                }
            }
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                let c1 = distort_point(c1x, c1y, distance_scale, frames, closed);
                let c2 = distort_point(c2x, c2y, distance_scale, frames, closed);
                let p = distort_point(x, y, distance_scale, frames, closed);
                PathCommand::CubicTo {
                    c1x: c1.x,
                    c1y: c1.y,
                    c2x: c2.x,
                    c2y: c2.y,
                    x: p.x,
                    y: p.y,
                }
            }
            PathCommand::Close => PathCommand::Close,
        };
        mapped.commands.push(next);
    }
    mapped
}

fn normalize_text_path(mut path: VecPath) -> VecPath {
    if let Some(bounds) = path.bounds() {
        let dx = bounds.min.x;
        for subpath in &mut path.subpaths {
            for cmd in &mut subpath.commands {
                match cmd {
                    PathCommand::MoveTo { x, .. } | PathCommand::LineTo { x, .. } => *x -= dx,
                    PathCommand::QuadTo { cx, x, .. } => {
                        *cx -= dx;
                        *x -= dx;
                    }
                    PathCommand::CubicTo { c1x, c2x, x, .. } => {
                        *c1x -= dx;
                        *c2x -= dx;
                        *x -= dx;
                    }
                    PathCommand::Close => {}
                }
            }
        }
    }
    path
}

pub fn layout_text_on_path(
    content: &str,
    font_family: &str,
    font_size_mm: f64,
    bold: bool,
    italic: bool,
    upper_case: bool,
    h_spacing: f64,
    path_offset: f64,
    rtl: bool,
    welded: bool,
    distort: bool,
    guide: &VecPath,
) -> Option<VecPath> {
    layout_text_on_path_resolved(
        content,
        font_family,
        font_size_mm,
        bold,
        italic,
        upper_case,
        h_spacing,
        path_offset,
        rtl,
        welded,
        distort,
        guide,
    )
    .map(|r| r.path)
}

/// Like [`layout_text_on_path`] but also returns the actual font resolution metadata
/// (source, key, missing_font) so callers can propagate truthful information.
pub(crate) fn layout_text_on_path_resolved(
    content: &str,
    font_family: &str,
    font_size_mm: f64,
    bold: bool,
    italic: bool,
    upper_case: bool,
    h_spacing: f64,
    path_offset: f64,
    rtl: bool,
    welded: bool,
    distort: bool,
    guide: &VecPath,
) -> Option<PathTextResult> {
    layout_text_on_path_resolved_with_options(
        content,
        font_family,
        font_size_mm,
        bold,
        italic,
        upper_case,
        h_spacing,
        path_offset,
        rtl,
        welded,
        distort,
        None,
        false,
        guide,
    )
}

pub(crate) fn layout_text_on_path_resolved_with_options(
    content: &str,
    font_family: &str,
    font_size_mm: f64,
    bold: bool,
    italic: bool,
    upper_case: bool,
    h_spacing: f64,
    path_offset: f64,
    rtl: bool,
    welded: bool,
    distort: bool,
    max_width: Option<f64>,
    squeeze: bool,
    guide: &VecPath,
) -> Option<PathTextResult> {
    layout_text_on_path_resolved_with_options_and_normal_offset(
        content,
        font_family,
        font_size_mm,
        bold,
        italic,
        upper_case,
        h_spacing,
        path_offset,
        rtl,
        welded,
        distort,
        max_width,
        squeeze,
        0.0,
        guide,
    )
}

pub(crate) fn layout_text_on_path_resolved_with_options_and_normal_offset(
    content: &str,
    font_family: &str,
    font_size_mm: f64,
    bold: bool,
    italic: bool,
    upper_case: bool,
    h_spacing: f64,
    path_offset: f64,
    rtl: bool,
    welded: bool,
    distort: bool,
    max_width: Option<f64>,
    squeeze: bool,
    normal_offset: f64,
    guide: &VecPath,
) -> Option<PathTextResult> {
    // Resolve text glyphs with properties — use Left alignment, no v_spacing
    // (single-line path text), and very large box so nothing clips.
    let resolved = resolve_text_in_box_with_options(
        content,
        font_family,
        font_size_mm,
        bold,
        italic,
        TextAlignment::Left,
        TextAlignmentV::Top,
        upper_case,
        h_spacing,
        0.0,
        f64::MAX / 4.0,
        f64::MAX / 4.0,
        max_width,
        squeeze,
        false,
    )?;
    let glyph_starts = resolved.glyph_starts;
    let font_source = resolved.resolved_font_source;
    let font_key = resolved.resolved_font_key;
    let missing_font = resolved.missing_font;
    let missing_glyphs = resolved.missing_glyphs;
    let mut text_path = normalize_text_path(resolved.path);

    // Translate the glyph baseline along the guide normal without mirroring it.
    // Circle placements use this to keep all four variants upright/readable.
    if normal_offset.is_finite() && normal_offset.abs() > f64::EPSILON {
        for subpath in &mut text_path.subpaths {
            for cmd in &mut subpath.commands {
                match cmd {
                    PathCommand::MoveTo { y, .. } | PathCommand::LineTo { y, .. } => {
                        *y += normal_offset;
                    }
                    PathCommand::QuadTo { cy, y, .. } => {
                        *cy += normal_offset;
                        *y += normal_offset;
                    }
                    PathCommand::CubicTo { c1y, c2y, y, .. } => {
                        *c1y += normal_offset;
                        *c2y += normal_offset;
                        *y += normal_offset;
                    }
                    PathCommand::Close => {}
                }
            }
        }
    }

    // Apply path_offset: shift all x-coordinates before mapping onto guide
    if path_offset.abs() > f64::EPSILON {
        for subpath in &mut text_path.subpaths {
            for cmd in &mut subpath.commands {
                match cmd {
                    PathCommand::MoveTo { x, .. } | PathCommand::LineTo { x, .. } => {
                        *x += path_offset;
                    }
                    PathCommand::QuadTo { cx, x, .. } => {
                        *cx += path_offset;
                        *x += path_offset;
                    }
                    PathCommand::CubicTo { c1x, c2x, x, .. } => {
                        *c1x += path_offset;
                        *c2x += path_offset;
                        *x += path_offset;
                    }
                    PathCommand::Close => {}
                }
            }
        }
    }

    // RTL: mirror glyph positions so text reads right-to-left along the path.
    // We reflect all x-coordinates around the text center so the first glyph
    // ends up at the rightmost position and the last glyph at the leftmost.
    if rtl {
        if let Some(bounds) = text_path.bounds() {
            let center_x = (bounds.min.x + bounds.max.x) / 2.0;
            for subpath in &mut text_path.subpaths {
                for cmd in &mut subpath.commands {
                    match cmd {
                        PathCommand::MoveTo { x, .. } | PathCommand::LineTo { x, .. } => {
                            *x = 2.0 * center_x - *x;
                        }
                        PathCommand::QuadTo { cx, x, .. } => {
                            *cx = 2.0 * center_x - *cx;
                            *x = 2.0 * center_x - *x;
                        }
                        PathCommand::CubicTo { c1x, c2x, x, .. } => {
                            *c1x = 2.0 * center_x - *c1x;
                            *c2x = 2.0 * center_x - *c2x;
                            *x = 2.0 * center_x - *x;
                        }
                        PathCommand::Close => {}
                    }
                }
            }
        }
    }

    let frames = path_frames(guide);
    if frames.is_empty() {
        return None;
    }

    let closed = guide_is_closed(guide);
    let text_width = text_path.bounds().map(|b| b.width()).unwrap_or(0.0);
    let guide_length = total_length(&frames);
    let distance_scale = if !closed && text_width > guide_length && text_width > f64::EPSILON {
        guide_length / text_width
    } else {
        1.0
    };

    // Build glyph ranges from glyph_starts for per-glyph-group mapping.
    // Each glyph group gets a single frame so multi-contour glyphs (O, B, e, 8)
    // have all their subpaths mapped with the same tangent/normal.
    let glyph_starts = &glyph_starts;
    let num_subpaths = text_path.subpaths.len();
    let glyph_ranges: Vec<std::ops::Range<usize>> = if glyph_starts.is_empty() {
        // Fallback: treat each subpath as its own group
        (0..num_subpaths).map(|i| i..i + 1).collect()
    } else {
        glyph_starts
            .windows(2)
            .map(|w| w[0]..w[1])
            .chain(std::iter::once(
                glyph_starts.last().copied().unwrap_or(0)..num_subpaths,
            ))
            .collect()
    };

    let mut mapped_subpaths = Vec::with_capacity(num_subpaths);
    for range in &glyph_ranges {
        if distort {
            // Per-point distortion: each coordinate gets its own frame sample
            for i in range.clone() {
                mapped_subpaths.push(distort_transform_subpath(
                    &text_path.subpaths[i],
                    distance_scale,
                    &frames,
                    closed,
                ));
            }
        } else {
            // Rigid-body: one frame per glyph group
            let (group_min_x, group_max_x) = range
                .clone()
                .map(|i| subpath_x_range(&text_path.subpaths[i]))
                .fold((f64::MAX, f64::MIN), |(mn, mx), (a, b)| {
                    (mn.min(a), mx.max(b))
                });
            if group_min_x > group_max_x {
                for i in range.clone() {
                    mapped_subpaths.push(text_path.subpaths[i].clone());
                }
                continue;
            }
            let center_x = (group_min_x + group_max_x) / 2.0;
            let guide_distance = center_x * distance_scale;

            if let Some((origin, tangent, normal)) = sample_frame(&frames, guide_distance, closed) {
                for i in range.clone() {
                    mapped_subpaths.push(rigid_transform_subpath(
                        &text_path.subpaths[i],
                        center_x,
                        &origin,
                        &tangent,
                        &normal,
                    ));
                }
            } else {
                for i in range.clone() {
                    mapped_subpaths.push(text_path.subpaths[i].clone());
                }
            }
        }
    }

    let result = VecPath {
        subpaths: mapped_subpaths,
    };

    let make_result = |path: VecPath| PathTextResult {
        path,
        resolved_font_source: font_source.clone(),
        resolved_font_key: font_key.clone(),
        missing_font,
        missing_glyphs: missing_glyphs.clone(),
    };

    // Welded: boolean-union adjacent glyphs into a single merged contour
    if welded {
        // Split into per-glyph subpaths (each closed subpath is a glyph contour)
        let glyph_paths: Vec<VecPath> = result
            .subpaths
            .iter()
            .map(|sp| VecPath {
                subpaths: vec![sp.clone()],
            })
            .collect();
        if !glyph_paths.is_empty() {
            return Some(make_result(weld_shapes(&glyph_paths)));
        }
    }

    Some(make_result(result))
}

pub fn apply_path_to_text_with_options(
    text_content: &str,
    font_family: &str,
    font_size_mm: f64,
    bold: bool,
    italic: bool,
    path: &VecPath,
) -> VecPath {
    layout_text_on_path(
        text_content,
        font_family,
        font_size_mm,
        bold,
        italic,
        false,
        0.0,
        0.0,
        false,
        false,
        false,
        path,
    )
    .unwrap_or_default()
}

/// Apply text along a path using the bundled backend font defaults.
pub fn apply_path_to_text(text_content: &str, path: &VecPath) -> VecPath {
    apply_path_to_text_with_options(text_content, "Liberation Sans", 10.0, false, false, path)
}

/// Generate a circular arc VecPath for bend-mode text layout.
/// Positive radius = convex (text curves away from baseline), negative = concave.
/// The arc length equals `text_width` and is centered horizontally.
pub(crate) fn generate_arc_guide(text_width: f64, radius: f64) -> VecPath {
    if text_width < f64::EPSILON || radius.abs() < f64::EPSILON {
        return VecPath::parse_svg_d(&format!("M0 0 L{text_width} 0"));
    }

    let r = radius.abs();
    let theta = text_width / r;
    let start_angle = -theta / 2.0;
    let cx = text_width / 2.0;

    // Arc parametrized by angle α from the baseline-nearest point:
    //   positive radius: center at (cx, -r), p(α) = (cx + r·sin(α), -r + r·cos(α))
    //   negative radius: center at (cx,  r), p(α) = (cx + r·sin(α),  r - r·cos(α))
    // At α=0: p = (cx, 0) — baseline center.
    let arc_point = |alpha: f64| -> (f64, f64) {
        if radius > 0.0 {
            (cx + r * alpha.sin(), -r + r * alpha.cos())
        } else {
            (cx + r * alpha.sin(), r - r * alpha.cos())
        }
    };

    let tangent_at = |a: f64| -> (f64, f64) {
        if radius > 0.0 {
            (r * a.cos(), -r * a.sin())
        } else {
            (r * a.cos(), r * a.sin())
        }
    };

    let num_segments = (theta / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let segment_angle = theta / num_segments as f64;
    let k = (4.0 / 3.0) * (segment_angle / 4.0).tan();

    let mut subpath = SubPath::new();
    let (sx, sy) = arc_point(start_angle);
    subpath.commands.push(PathCommand::MoveTo { x: sx, y: sy });

    for seg_i in 0..num_segments {
        let a0 = start_angle + seg_i as f64 * segment_angle;
        let a1 = a0 + segment_angle;

        let (p0x, p0y) = arc_point(a0);
        let (p1x, p1y) = arc_point(a1);
        let (t0x, t0y) = tangent_at(a0);
        let (t1x, t1y) = tangent_at(a1);

        subpath.commands.push(PathCommand::CubicTo {
            c1x: p0x + k * t0x,
            c1y: p0y + k * t0y,
            c2x: p1x - k * t1x,
            c2y: p1y - k * t1y,
            x: p1x,
            y: p1y,
        });
    }

    VecPath {
        subpaths: vec![subpath],
    }
}

/// Generate a readable top/bottom circular guide for the Circle text transform.
///
/// Top and bottom guides both run visual left-to-right at their midpoint. The
/// path layout's normal offset places glyphs inside/outside without mirroring them.
pub(crate) fn generate_circle_guide(
    text_width: f64,
    font_size_mm: f64,
    curve: f64,
    placement: TextCirclePlacement,
) -> VecPath {
    if !text_width.is_finite() || text_width <= f64::EPSILON {
        return VecPath::default();
    }

    let (sweep, radius) = circle_guide_metrics(text_width, font_size_mm, curve);

    let (start_angle, signed_sweep) = match placement {
        TextCirclePlacement::TopOutside | TextCirclePlacement::TopInside => {
            (-std::f64::consts::FRAC_PI_2 - sweep / 2.0, sweep)
        }
        TextCirclePlacement::BottomOutside | TextCirclePlacement::BottomInside => {
            (std::f64::consts::FRAC_PI_2 + sweep / 2.0, -sweep)
        }
    };
    generate_circular_arc(radius, start_angle, signed_sweep)
}

/// Offset that centers the intrinsic text run on the generated circular guide.
pub(crate) fn circle_guide_path_offset(text_width: f64, font_size_mm: f64, curve: f64) -> f64 {
    let (sweep, radius) = circle_guide_metrics(text_width, font_size_mm, curve);
    ((radius * sweep - text_width) / 2.0).max(0.0)
}

fn circle_guide_metrics(text_width: f64, font_size_mm: f64, curve: f64) -> (f64, f64) {
    let curve = if curve.is_finite() {
        curve.clamp(-100.0, 100.0)
    } else {
        0.0
    };
    // Circle remains active at curve=0. The slider varies the occupied sweep
    // from a subtle quarter-circle to a strong 315-degree wrap.
    let sweep = std::f64::consts::PI * (1.0 + 0.75 * curve / 100.0);
    let font_height = if font_size_mm.is_finite() {
        font_size_mm.abs().clamp(0.01, 1_000_000.0)
    } else {
        10.0
    };
    let radius = (text_width / sweep)
        .max(font_height * 0.75)
        .clamp(0.01, 1_000_000.0);
    (sweep, radius)
}

fn generate_circular_arc(radius: f64, start_angle: f64, sweep: f64) -> VecPath {
    if !radius.is_finite()
        || radius <= f64::EPSILON
        || !start_angle.is_finite()
        || !sweep.is_finite()
        || sweep.abs() <= f64::EPSILON
    {
        return VecPath::default();
    }

    let num_segments = (sweep.abs() / std::f64::consts::FRAC_PI_2)
        .ceil()
        .clamp(1.0, 8.0) as usize;
    let segment_angle = sweep / num_segments as f64;
    let mut subpath = SubPath::new();
    subpath.commands.push(PathCommand::MoveTo {
        x: radius * start_angle.cos(),
        y: radius * start_angle.sin(),
    });

    for segment in 0..num_segments {
        let a0 = start_angle + segment as f64 * segment_angle;
        let a1 = a0 + segment_angle;
        let k = (4.0 / 3.0) * (segment_angle / 4.0).tan();
        let (p0x, p0y) = (radius * a0.cos(), radius * a0.sin());
        let (p1x, p1y) = (radius * a1.cos(), radius * a1.sin());
        let (t0x, t0y) = (-radius * a0.sin(), radius * a0.cos());
        let (t1x, t1y) = (-radius * a1.sin(), radius * a1.cos());
        subpath.commands.push(PathCommand::CubicTo {
            c1x: p0x + k * t0x,
            c1y: p0y + k * t0y,
            c2x: p1x - k * t1x,
            c2y: p1y - k * t1y,
            x: p1x,
            y: p1y,
        });
    }

    VecPath {
        subpaths: vec![subpath],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_path_to_text_returns_geometry() {
        let path = VecPath::parse_svg_d("M0 0 L100 0");
        let result = apply_path_to_text("Hello", &path);

        assert!(!result.is_empty());
        assert_ne!(result.to_svg_d(), path.to_svg_d());
    }

    #[test]
    fn layout_text_on_curve_produces_nontrivial_bounds() {
        let guide = VecPath::parse_svg_d("M0 0 C25 30 75 -30 100 0");
        let result = layout_text_on_path(
            "Curve",
            "Liberation Sans",
            10.0,
            false,
            false,
            false,
            0.0,
            0.0,
            false,
            false,
            false,
            &guide,
        )
        .unwrap();
        let bounds = result.bounds().unwrap();
        assert!(bounds.width() > 10.0);
        assert!(bounds.height() > 1.0);
    }

    #[test]
    fn layout_text_on_closed_path_wraps() {
        let guide = VecPath::parse_svg_d("M0 0 L50 0 L50 50 L0 50 Z");
        let result = layout_text_on_path(
            "Wrap",
            "Liberation Sans",
            12.0,
            false,
            false,
            false,
            0.0,
            0.0,
            false,
            false,
            false,
            &guide,
        )
        .unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn empty_guide_returns_none() {
        let guide = VecPath::default();
        assert!(
            layout_text_on_path(
                "Hello",
                "Liberation Sans",
                10.0,
                false,
                false,
                false,
                0.0,
                0.0,
                false,
                false,
                false,
                &guide
            )
            .is_none()
        );
    }

    #[test]
    fn path_text_respects_upper_case() {
        let guide = VecPath::parse_svg_d("M0 0 L200 0");
        let lower = layout_text_on_path(
            "hello",
            "Liberation Sans",
            10.0,
            false,
            false,
            false,
            0.0,
            0.0,
            false,
            false,
            false,
            &guide,
        )
        .unwrap();
        let upper = layout_text_on_path(
            "hello",
            "Liberation Sans",
            10.0,
            false,
            false,
            true,
            0.0,
            0.0,
            false,
            false,
            false,
            &guide,
        )
        .unwrap();
        // Upper case should produce different path data (different glyphs)
        assert_ne!(
            lower.to_svg_d(),
            upper.to_svg_d(),
            "upper_case should change path-text geometry"
        );
    }

    #[test]
    fn path_text_respects_h_spacing() {
        let guide = VecPath::parse_svg_d("M0 0 L200 0");
        let normal = layout_text_on_path(
            "hello",
            "Liberation Sans",
            10.0,
            false,
            false,
            false,
            0.0,
            0.0,
            false,
            false,
            false,
            &guide,
        )
        .unwrap();
        let spaced = layout_text_on_path(
            "hello",
            "Liberation Sans",
            10.0,
            false,
            false,
            false,
            2.0,
            0.0,
            false,
            false,
            false,
            &guide,
        )
        .unwrap();
        let normal_w = normal.bounds().unwrap().width();
        let spaced_w = spaced.bounds().unwrap().width();
        assert!(
            spaced_w > normal_w,
            "h_spacing=2.0 should produce wider text: {spaced_w} vs {normal_w}"
        );
    }

    #[test]
    fn path_text_respects_path_offset() {
        let guide = VecPath::parse_svg_d("M0 0 L200 0");
        let no_offset = layout_text_on_path(
            "Hi",
            "Liberation Sans",
            10.0,
            false,
            false,
            false,
            0.0,
            0.0,
            false,
            false,
            false,
            &guide,
        )
        .unwrap();
        let with_offset = layout_text_on_path(
            "Hi",
            "Liberation Sans",
            10.0,
            false,
            false,
            false,
            0.0,
            20.0,
            false,
            false,
            false,
            &guide,
        )
        .unwrap();
        let no_b = no_offset.bounds().unwrap();
        let off_b = with_offset.bounds().unwrap();
        // On a horizontal guide, path_offset shifts text along x
        assert!(
            off_b.min.x > no_b.min.x + 10.0,
            "path_offset=20 should shift text right: {:.1} vs {:.1}",
            off_b.min.x,
            no_b.min.x
        );
    }

    #[test]
    fn path_frames_empty_closed_polyline_no_panic() {
        // A VecPath with a single Close command (no MoveTo/LineTo) produces
        // an empty closed polyline when flattened. path_frames must not panic.
        let guide = VecPath {
            subpaths: vec![SubPath {
                commands: vec![PathCommand::Close],
                closed: true,
            }],
        };
        let frames = path_frames(&guide);
        assert!(frames.is_empty());
    }

    #[test]
    fn path_text_rtl_reverses_glyph_order() {
        let guide = VecPath::parse_svg_d("M0 0 L200 0");
        let ltr = layout_text_on_path(
            "AB",
            "Liberation Sans",
            10.0,
            false,
            false,
            false,
            0.0,
            0.0,
            false,
            false,
            false,
            &guide,
        )
        .unwrap();
        let rtl = layout_text_on_path(
            "AB",
            "Liberation Sans",
            10.0,
            false,
            false,
            false,
            0.0,
            0.0,
            true,
            false,
            false,
            &guide,
        )
        .unwrap();
        // RTL should produce different geometry (mirrored glyph positions)
        assert_ne!(
            ltr.to_svg_d(),
            rtl.to_svg_d(),
            "rtl should produce different path-text geometry"
        );
        // Both should have the same total width (mirroring preserves extent)
        let ltr_w = ltr.bounds().unwrap().width();
        let rtl_w = rtl.bounds().unwrap().width();
        assert!(
            (ltr_w - rtl_w).abs() < 1.0,
            "rtl and ltr should have similar width: {ltr_w} vs {rtl_w}"
        );
    }

    #[test]
    fn path_text_welded_merges_glyphs() {
        let guide = VecPath::parse_svg_d("M0 0 L200 0");
        let normal = layout_text_on_path(
            "MM",
            "Liberation Sans",
            10.0,
            false,
            false,
            false,
            0.0,
            0.0,
            false,
            false,
            false,
            &guide,
        )
        .unwrap();
        let welded = layout_text_on_path(
            "MM",
            "Liberation Sans",
            10.0,
            false,
            false,
            false,
            0.0,
            0.0,
            false,
            true,
            false,
            &guide,
        )
        .unwrap();
        // Welded should merge overlapping contours — fewer or different subpaths
        assert_ne!(
            normal.to_svg_d(),
            welded.to_svg_d(),
            "welded should change path-text geometry"
        );
        // Welded should have fewer or equal subpaths (union merges touching glyphs)
        assert!(
            welded.subpaths.len() <= normal.subpaths.len(),
            "welded should have <= subpaths: {} vs {}",
            welded.subpaths.len(),
            normal.subpaths.len()
        );
    }

    #[test]
    fn path_text_multi_contour_glyph_consistent_frame() {
        // "O" has 2 contours (outer + inner hole). On a curve, both contours
        // must share the same guide frame so the glyph stays aligned.
        let guide = VecPath::parse_svg_d("M0 0 C50 40 100 -40 150 0");
        let result = layout_text_on_path(
            "O",
            "Liberation Sans",
            10.0,
            false,
            false,
            false,
            0.0,
            0.0,
            false,
            false,
            false,
            &guide,
        )
        .unwrap();
        assert!(
            result.subpaths.len() >= 2,
            "O should have at least 2 subpaths (outer + inner): got {}",
            result.subpaths.len()
        );
        // Compute center of each subpath's bounds
        let centers: Vec<(f64, f64)> = result
            .subpaths
            .iter()
            .filter_map(|sp| {
                let (min_x, max_x) = subpath_x_range(sp);
                let path = VecPath {
                    subpaths: vec![sp.clone()],
                };
                let b = path.bounds()?;
                Some(((min_x + max_x) / 2.0, (b.min.y + b.max.y) / 2.0))
            })
            .collect();
        // All contour centers should be within 0.1mm of each other
        for pair in centers.windows(2) {
            let dx = (pair[0].0 - pair[1].0).abs();
            let dy = (pair[0].1 - pair[1].1).abs();
            assert!(
                dx < 0.1 && dy < 0.1,
                "Multi-contour glyph centers should match: ({:.3},{:.3}) vs ({:.3},{:.3}), delta ({:.3},{:.3})",
                pair[0].0,
                pair[0].1,
                pair[1].0,
                pair[1].1,
                dx,
                dy
            );
        }
    }

    #[test]
    fn path_text_adjacent_glyphs_separate_frames() {
        // "OO" — two adjacent O glyphs should get different positions on the guide
        let guide = VecPath::parse_svg_d("M0 0 L200 0");
        let result = layout_text_on_path(
            "OO",
            "Liberation Sans",
            10.0,
            false,
            false,
            false,
            2.0,
            0.0,
            false,
            false,
            false,
            &guide,
        )
        .unwrap();
        // With h_spacing=2.0, the two O's should be clearly separated
        let bounds = result.bounds().unwrap();
        // Each O has ~2 subpaths, so we should have at least 4
        assert!(
            result.subpaths.len() >= 4,
            "OO should have >= 4 subpaths: got {}",
            result.subpaths.len()
        );
        // The first O's subpaths should be on the left, second O's on the right
        let first_o_center = {
            let (mn, mx) = subpath_x_range(&result.subpaths[0]);
            (mn + mx) / 2.0
        };
        // Find the start of second O — it should be further right
        let second_o_center = {
            let (mn, mx) = subpath_x_range(&result.subpaths[2]);
            (mn + mx) / 2.0
        };
        assert!(
            second_o_center > first_o_center + 1.0,
            "Adjacent glyphs should have different positions: {first_o_center:.1} vs {second_o_center:.1}"
        );
        assert!(bounds.width() > 10.0, "OO should have reasonable width");
    }

    #[test]
    fn path_text_zero_spacing_uses_shaping_boundaries() {
        // At zero h_spacing (tightest supported), glyph boundaries come from
        // the shaping step (glyph_starts), not from x-overlap detection.
        // Each glyph must still get its own frame.
        let guide = VecPath::parse_svg_d("M0 0 C50 30 100 -30 150 0");
        let result = layout_text_on_path(
            "AB",
            "Liberation Sans",
            10.0,
            false,
            false,
            false,
            0.0,
            0.0,
            false,
            false,
            false,
            &guide,
        )
        .unwrap();
        assert!(!result.is_empty());
        // Verify that A and B are at different positions along the guide.
        // Each has at least 1 subpath; the first subpath of each glyph should
        // have a different x-center (they occupy different guide positions).
        let a_center = {
            let (mn, mx) = subpath_x_range(&result.subpaths[0]);
            (mn + mx) / 2.0
        };
        let last_sp = result.subpaths.last().unwrap();
        let b_center = {
            let (mn, mx) = subpath_x_range(last_sp);
            (mn + mx) / 2.0
        };
        assert!(
            (a_center - b_center).abs() > 1.0,
            "Zero-spacing glyphs should still have separate guide positions: A={a_center:.1}, B={b_center:.1}"
        );
    }

    #[test]
    fn smooth_tangent_on_flattened_cubic() {
        // A cubic bezier flattens into many short segments. Tangent angles
        // between consecutive sample points should change gradually (< 5° per step).
        let guide = VecPath::parse_svg_d("M0 0 C30 40 70 -40 100 0");
        let frames = path_frames(&guide);
        assert!(!frames.is_empty());
        let total = total_length(&frames);

        let num_samples = 40;
        let mut prev_tangent: Option<Point2D> = None;
        let mut max_angle_deg = 0.0f64;
        for i in 0..num_samples {
            let d = total * (i as f64) / (num_samples as f64 - 1.0);
            if let Some((_, tangent, _)) = sample_frame(&frames, d, false) {
                if let Some(prev) = &prev_tangent {
                    let dot = (prev.x * tangent.x + prev.y * tangent.y).clamp(-1.0, 1.0);
                    let angle_deg = dot.acos().to_degrees();
                    max_angle_deg = max_angle_deg.max(angle_deg);
                }
                prev_tangent = Some(tangent);
            }
        }
        assert!(
            max_angle_deg < 10.0,
            "Max tangent angle change between consecutive samples should be < 10°, got {max_angle_deg:.2}°"
        );
    }

    #[test]
    fn no_smoothing_across_true_sharp_corner() {
        // L-shaped guide with a 90° corner — smoothing must NOT blend
        let guide = VecPath::parse_svg_d("M0 0 L50 0 L50 50");
        let frames = path_frames(&guide);

        // Sample just before and after the corner (at distance 50)
        let before = sample_frame(&frames, 49.9, false).unwrap();
        let after = sample_frame(&frames, 50.1, false).unwrap();
        // Before corner: tangent should be ~horizontal (1, 0)
        assert!(
            before.1.x > 0.9,
            "before corner tangent.x should be ~1.0: {:.3}",
            before.1.x
        );
        assert!(
            before.1.y.abs() < 0.1,
            "before corner tangent.y should be ~0: {:.3}",
            before.1.y
        );
        // After corner: tangent should be ~vertical (0, 1)
        assert!(
            after.1.x.abs() < 0.1,
            "after corner tangent.x should be ~0: {:.3}",
            after.1.x
        );
        assert!(
            after.1.y > 0.9,
            "after corner tangent.y should be ~1.0: {:.3}",
            after.1.y
        );
    }

    #[test]
    fn no_smoothing_across_shallow_polyline_corner() {
        // V-shaped polyline at ~20° bend — dot product ≈ cos(20°) ≈ 0.94 < 0.98 threshold
        // so smoothing should NOT apply
        let guide = VecPath::parse_svg_d("M0 0 L50 0 L100 17.6");
        // angle of second segment: atan2(17.6, 50) ≈ 19.4°
        let frames = path_frames(&guide);

        let before = sample_frame(&frames, 49.9, false).unwrap();
        let after = sample_frame(&frames, 50.1, false).unwrap();
        // Before: horizontal tangent
        assert!(before.1.x > 0.99, "before should be horizontal");
        // After: should have the tilted tangent, NOT a blended one
        let expected_angle = (17.6_f64).atan2(50.0);
        let actual_angle = after.1.y.atan2(after.1.x);
        let diff = (expected_angle - actual_angle).abs().to_degrees();
        assert!(
            diff < 2.0,
            "shallow polyline corner should NOT be smoothed, angle diff: {diff:.1}°"
        );
    }

    #[test]
    fn tight_curve_text_bounds_are_smooth() {
        // Text on a tight cubic curve (small radius ~20mm) should follow the curve closely
        let guide = VecPath::parse_svg_d("M0 20 C10 0 30 0 40 20");
        let result = layout_text_on_path(
            "AB",
            "Liberation Sans",
            5.0,
            false,
            false,
            false,
            0.0,
            0.0,
            false,
            false,
            false,
            &guide,
        )
        .unwrap();
        let bounds = result.bounds().unwrap();
        // The text should have reasonable bounds — not degenerate
        assert!(
            bounds.width() > 3.0,
            "tight curve text width: {:.1}",
            bounds.width()
        );
        assert!(
            bounds.height() > 1.0,
            "tight curve text height: {:.1}",
            bounds.height()
        );
    }

    #[test]
    fn distort_on_curve_differs_from_rigid() {
        let guide = VecPath::parse_svg_d("M0 0 C30 40 70 -40 100 0");
        let rigid = layout_text_on_path(
            "Hello",
            "Liberation Sans",
            10.0,
            false,
            false,
            false,
            0.0,
            0.0,
            false,
            false,
            false,
            &guide,
        )
        .unwrap();
        let distorted = layout_text_on_path(
            "Hello",
            "Liberation Sans",
            10.0,
            false,
            false,
            false,
            0.0,
            0.0,
            false,
            false,
            true,
            &guide,
        )
        .unwrap();
        assert_ne!(
            rigid.to_svg_d(),
            distorted.to_svg_d(),
            "distort should produce different geometry"
        );
    }

    #[test]
    fn distort_on_straight_line_matches_rigid() {
        let guide = VecPath::parse_svg_d("M0 0 L200 0");
        let rigid = layout_text_on_path(
            "Test",
            "Liberation Sans",
            10.0,
            false,
            false,
            false,
            0.0,
            0.0,
            false,
            false,
            false,
            &guide,
        )
        .unwrap();
        let distorted = layout_text_on_path(
            "Test",
            "Liberation Sans",
            10.0,
            false,
            false,
            false,
            0.0,
            0.0,
            false,
            false,
            true,
            &guide,
        )
        .unwrap();
        let rigid_b = rigid.bounds().unwrap();
        let dist_b = distorted.bounds().unwrap();
        assert!(
            (rigid_b.width() - dist_b.width()).abs() < 0.1,
            "straight line: distort width {:.3} vs rigid {:.3}",
            dist_b.width(),
            rigid_b.width()
        );
        assert!(
            (rigid_b.height() - dist_b.height()).abs() < 0.1,
            "straight line: distort height {:.3} vs rigid {:.3}",
            dist_b.height(),
            rigid_b.height()
        );
    }

    #[test]
    fn distort_multi_contour_stays_coherent() {
        // "O" on curved guide with distort: inner and outer contour bounds should overlap
        let guide = VecPath::parse_svg_d("M0 0 C50 40 100 -40 150 0");
        let result = layout_text_on_path(
            "O",
            "Liberation Sans",
            10.0,
            false,
            false,
            false,
            0.0,
            0.0,
            false,
            false,
            true,
            &guide,
        )
        .unwrap();
        assert!(result.subpaths.len() >= 2, "O should have >= 2 subpaths");
        // All contour bounds should overlap (hole stays inside glyph)
        let all_bounds: Vec<_> = result
            .subpaths
            .iter()
            .filter_map(|sp| {
                VecPath {
                    subpaths: vec![sp.clone()],
                }
                .bounds()
            })
            .collect();
        for pair in all_bounds.windows(2) {
            // Bounds should overlap: max of mins < min of maxes in at least one axis
            let overlap_x = pair[0].max.x > pair[1].min.x && pair[1].max.x > pair[0].min.x;
            let overlap_y = pair[0].max.y > pair[1].min.y && pair[1].max.y > pair[0].min.y;
            assert!(
                overlap_x && overlap_y,
                "contour bounds should overlap with distort"
            );
        }
    }

    #[test]
    fn distort_plus_welded_works() {
        let guide = VecPath::parse_svg_d("M0 0 C50 30 100 -30 150 0");
        let result = layout_text_on_path(
            "MM",
            "Liberation Sans",
            10.0,
            false,
            false,
            false,
            0.0,
            0.0,
            false,
            true,
            true,
            &guide,
        );
        assert!(result.is_some(), "distort + welded should produce geometry");
        let path = result.unwrap();
        assert!(!path.is_empty());
    }

    #[test]
    fn generate_arc_guide_length_matches_width() {
        use crate::vector::flatten::flatten_vecpath;
        let text_width = 80.0;
        let radius = 50.0;
        let arc = generate_arc_guide(text_width, radius);
        // Flatten and sum segment lengths
        let polys = flatten_vecpath(&arc, 0.01);
        let total_len: f64 = polys
            .iter()
            .map(|poly| {
                poly.points
                    .windows(2)
                    .map(|w| w[0].distance_to(&w[1]))
                    .sum::<f64>()
            })
            .sum();
        let error_pct = ((total_len - text_width) / text_width).abs() * 100.0;
        assert!(
            error_pct < 1.0,
            "arc length {total_len:.3} should match text_width {text_width:.3} within 1%, got {error_pct:.2}%"
        );
    }

    #[test]
    fn circle_guides_use_readable_traversal_for_all_placements() {
        let cases = [
            (TextCirclePlacement::TopOutside, -1.0, 1.0, 1.0),
            (TextCirclePlacement::TopInside, -1.0, 1.0, 1.0),
            (TextCirclePlacement::BottomOutside, 1.0, 1.0, 1.0),
            (TextCirclePlacement::BottomInside, 1.0, 1.0, 1.0),
        ];

        for (placement, expected_y, expected_tangent_x, expected_normal_y) in cases {
            let guide = generate_circle_guide(60.0, 10.0, 0.0, placement);
            let frames = path_frames(&guide);
            let (origin, tangent, normal) =
                sample_frame(&frames, total_length(&frames) / 2.0, false).unwrap();
            assert_eq!(origin.y.signum(), expected_y, "{placement:?} vertical half");
            assert_eq!(
                tangent.x.signum(),
                expected_tangent_x,
                "{placement:?} reading direction"
            );
            assert_eq!(
                normal.y.signum(),
                expected_normal_y,
                "{placement:?} glyph baseline side"
            );
        }
    }
}

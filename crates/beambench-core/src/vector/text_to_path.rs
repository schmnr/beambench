use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

use beambench_common::geometry::Bounds;
use beambench_common::path::{PathCommand, SubPath, VecPath};
use fontdb::{Database, Family, ID, Query, Stretch, Style, Weight};
use ttf_parser::{Face, GlyphId, OutlineBuilder};

use crate::object::{
    ObjectData, TextAlignment, TextAlignmentV, TextCirclePlacement, TextFontSource, TextLayoutMode,
    TextTransformStyle,
};
use crate::variable_text::{self, VariableTextConfig, VariableTextMode};

/// Bundled Liberation Sans Regular font used as fallback text geometry.
const LIBERATION_SANS: &[u8] = include_bytes!("../../../fonts/LiberationSans-Regular.ttf");

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTextPath {
    pub path: VecPath,
    /// Subpath index where each glyph begins (used for per-glyph-group mapping on path-text).
    pub glyph_starts: Vec<usize>,
    pub resolved_font_source: TextFontSource,
    pub resolved_font_key: String,
    pub missing_font: bool,
    pub missing_glyphs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ResolvedGlyphFace {
    System(ID),
    BundledFallback,
}

#[derive(Debug, Clone)]
struct ResolvedGlyph {
    face: Option<ResolvedGlyphFace>,
    glyph_id: Option<GlyphId>,
    advance_mm: f64,
}

struct GlyphPathBuilder {
    subpaths: Vec<SubPath>,
    current: SubPath,
    scale_x: f64,
    scale_y: f64,
    offset_x: f64,
    baseline_y: f64,
}

impl GlyphPathBuilder {
    fn new(scale_x: f64, scale_y: f64, offset_x: f64, baseline_y: f64) -> Self {
        Self {
            subpaths: Vec::new(),
            current: SubPath::new(),
            scale_x,
            scale_y,
            offset_x,
            baseline_y,
        }
    }

    fn finish(mut self) -> Vec<SubPath> {
        if !self.current.commands.is_empty() {
            self.subpaths.push(self.current);
        }
        self.subpaths
    }

    fn push_point(&self, x: f32, y: f32) -> (f64, f64) {
        (
            self.offset_x + x as f64 * self.scale_x,
            self.baseline_y - y as f64 * self.scale_y,
        )
    }

    fn flush_current(&mut self) {
        if !self.current.commands.is_empty() {
            self.subpaths.push(std::mem::take(&mut self.current));
        }
    }
}

impl OutlineBuilder for GlyphPathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.flush_current();
        let (x, y) = self.push_point(x, y);
        self.current.commands.push(PathCommand::MoveTo { x, y });
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.push_point(x, y);
        self.current.commands.push(PathCommand::LineTo { x, y });
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (cx, cy) = self.push_point(x1, y1);
        let (x, y) = self.push_point(x, y);
        self.current
            .commands
            .push(PathCommand::QuadTo { cx, cy, x, y });
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (c1x, c1y) = self.push_point(x1, y1);
        let (c2x, c2y) = self.push_point(x2, y2);
        let (x, y) = self.push_point(x, y);
        self.current.commands.push(PathCommand::CubicTo {
            c1x,
            c1y,
            c2x,
            c2y,
            x,
            y,
        });
    }

    fn close(&mut self) {
        self.current.commands.push(PathCommand::Close);
        self.current.closed = true;
        self.flush_current();
    }
}

fn system_db() -> &'static Database {
    static DB: OnceLock<Database> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = Database::new();
        db.load_system_fonts();
        db
    })
}

fn is_generic_font_family(font_family: &str) -> bool {
    matches!(
        font_family.to_ascii_lowercase().as_str(),
        "sans-serif" | "sans serif" | "serif" | "monospace" | "cursive" | "fantasy"
    )
}

/// Check whether a given font family (with bold/italic) can be resolved from
/// the system font database. Returns true if the font is available.
pub fn can_resolve_font(font_family: &str, bold: bool, italic: bool) -> bool {
    let db = system_db();
    let generic = match font_family.to_ascii_lowercase().as_str() {
        "sans-serif" | "sans serif" => Some(Family::SansSerif),
        "serif" => Some(Family::Serif),
        "monospace" => Some(Family::Monospace),
        "cursive" => Some(Family::Cursive),
        "fantasy" => Some(Family::Fantasy),
        _ => None,
    };
    let families = if let Some(generic) = generic {
        [generic, Family::Name(font_family)]
    } else {
        [Family::Name(font_family), Family::SansSerif]
    };
    let query = Query {
        families: &families,
        weight: if bold { Weight::BOLD } else { Weight::NORMAL },
        stretch: Stretch::Normal,
        style: if italic { Style::Italic } else { Style::Normal },
    };
    db.query(&query).is_some() || (generic.is_some() && fallback_face().is_some())
}

fn fallback_face() -> Option<Face<'static>> {
    Face::parse(LIBERATION_SANS, 0).ok()
}

fn glyph_advance(face: &Face<'_>, glyph_id: GlyphId, scale_x: f64, letter_spacing: f64) -> f64 {
    face.glyph_hor_advance(glyph_id)
        .map(|advance| advance as f64 * scale_x + letter_spacing)
        .unwrap_or(letter_spacing)
}

fn glyph_has_outline(face: &Face<'_>, glyph_id: GlyphId) -> bool {
    face.outline_glyph(glyph_id, &mut GlyphPathBuilder::new(1.0, 1.0, 0.0, 0.0))
        .is_some()
}

fn system_face_glyph(
    db: &Database,
    id: ID,
    ch: char,
    font_size_mm: f64,
    require_outline: bool,
) -> Option<(GlyphId, f64)> {
    db.with_face_data(id, |data, index| {
        let face = Face::parse(data, index).ok()?;
        let glyph_id = face.glyph_index(ch)?;
        if require_outline && !glyph_has_outline(&face, glyph_id) {
            return None;
        }
        let units_per_em = face.units_per_em() as f64;
        let advance = face
            .glyph_hor_advance(glyph_id)
            .map(|advance| advance as f64 / units_per_em * font_size_mm)
            .unwrap_or(font_size_mm * 0.5);
        Some((glyph_id, advance))
    })?
}

fn fallback_face_glyph(
    ch: char,
    font_size_mm: f64,
    require_outline: bool,
) -> Option<(GlyphId, f64)> {
    let face = fallback_face()?;
    let glyph_id = face.glyph_index(ch)?;
    if require_outline && !glyph_has_outline(&face, glyph_id) {
        return None;
    }
    let units_per_em = face.units_per_em() as f64;
    let advance = face
        .glyph_hor_advance(glyph_id)
        .map(|advance| advance as f64 / units_per_em * font_size_mm)
        .unwrap_or(font_size_mm * 0.5);
    Some((glyph_id, advance))
}

fn fallback_face_for_char(
    db: &Database,
    primary_id: ID,
    ch: char,
    font_size_mm: f64,
) -> Option<ResolvedGlyphFace> {
    let require_outline = !ch.is_whitespace();
    for face_info in db.faces() {
        let id = face_info.id;
        if id == primary_id {
            continue;
        }
        if system_face_glyph(db, id, ch, font_size_mm, require_outline).is_some() {
            return Some(ResolvedGlyphFace::System(id));
        }
    }
    if fallback_face_glyph(ch, font_size_mm, require_outline).is_some() {
        return Some(ResolvedGlyphFace::BundledFallback);
    }
    None
}

fn resolve_system_glyph(
    db: &Database,
    primary_id: ID,
    ch: char,
    font_size_mm: f64,
    cache: &mut HashMap<char, Option<ResolvedGlyphFace>>,
    missing_glyphs: &mut BTreeSet<String>,
) -> ResolvedGlyph {
    let require_outline = !ch.is_whitespace();
    if let Some((glyph_id, advance_mm)) =
        system_face_glyph(db, primary_id, ch, font_size_mm, require_outline)
    {
        return ResolvedGlyph {
            face: Some(ResolvedGlyphFace::System(primary_id)),
            glyph_id: Some(glyph_id),
            advance_mm,
        };
    }

    let fallback = if let Some(cached) = cache.get(&ch) {
        *cached
    } else {
        let resolved = fallback_face_for_char(db, primary_id, ch, font_size_mm);
        cache.insert(ch, resolved);
        resolved
    };

    match fallback {
        Some(ResolvedGlyphFace::System(id)) => {
            if let Some((glyph_id, advance_mm)) =
                system_face_glyph(db, id, ch, font_size_mm, require_outline)
            {
                return ResolvedGlyph {
                    face: Some(ResolvedGlyphFace::System(id)),
                    glyph_id: Some(glyph_id),
                    advance_mm,
                };
            }
        }
        Some(ResolvedGlyphFace::BundledFallback) => {
            if let Some((glyph_id, advance_mm)) =
                fallback_face_glyph(ch, font_size_mm, require_outline)
            {
                return ResolvedGlyph {
                    face: Some(ResolvedGlyphFace::BundledFallback),
                    glyph_id: Some(glyph_id),
                    advance_mm,
                };
            }
        }
        None => {}
    }

    if !ch.is_whitespace() {
        missing_glyphs.insert(ch.to_string());
    }
    ResolvedGlyph {
        face: None,
        glyph_id: None,
        advance_mm: if ch.is_whitespace() {
            font_size_mm * 0.33
        } else {
            font_size_mm * 0.6
        },
    }
}

fn resolve_system_line_glyphs(
    db: &Database,
    primary_id: ID,
    text: &str,
    font_size_mm: f64,
    cache: &mut HashMap<char, Option<ResolvedGlyphFace>>,
    missing_glyphs: &mut BTreeSet<String>,
) -> Vec<ResolvedGlyph> {
    text.chars()
        .map(|ch| resolve_system_glyph(db, primary_id, ch, font_size_mm, cache, missing_glyphs))
        .collect()
}

fn resolved_line_width(glyphs: &[ResolvedGlyph], letter_spacing: f64) -> f64 {
    let glyph_count = glyphs.len();
    let advance: f64 = glyphs.iter().map(|glyph| glyph.advance_mm).sum();
    if glyph_count > 1 {
        advance + letter_spacing * (glyph_count - 1) as f64
    } else {
        advance
    }
}

fn line_width(face: &Face<'_>, text: &str, scale_x: f64, letter_spacing: f64) -> f64 {
    let mut width = 0.0;
    let mut glyph_count = 0usize;
    for ch in text.chars() {
        if let Some(glyph_id) = face.glyph_index(ch) {
            width += face
                .glyph_hor_advance(glyph_id)
                .map(|advance| advance as f64 * scale_x)
                .unwrap_or(0.0);
            glyph_count += 1;
        }
    }
    if glyph_count > 1 {
        width += letter_spacing * (glyph_count - 1) as f64;
    }
    width
}

fn normalize_text_whitespace(text: String) -> String {
    let mut normalized = text.replace(" \n", "\n").replace("\n ", "\n");
    while normalized.contains("  ") {
        normalized = normalized.replace("  ", " ");
    }
    normalized
}

fn strip_unresolved_merge_fields(text: &str) -> String {
    let fields = variable_text::parse_merge_fields(text);
    if fields.is_empty() {
        return text.to_string();
    }
    let mut result = String::with_capacity(text.len());
    let mut last_end = 0;
    for field in &fields {
        result.push_str(&text[last_end..field.start]);
        last_end = field.end;
    }
    result.push_str(&text[last_end..]);
    normalize_text_whitespace(result)
}

fn resolve_text_content_for_object(
    content: &str,
    variable_text: Option<&VariableTextConfig>,
    ignore_empty_vars: bool,
) -> String {
    let resolved = if let Some(config) = variable_text {
        if matches!(config.mode, Some(VariableTextMode::Normal)) {
            config.template.clone()
        } else {
            let mut source = config.source.clone();
            if source.end < source.start {
                std::mem::swap(&mut source.start, &mut source.end);
            }
            if matches!(
                config.mode,
                None | Some(VariableTextMode::SerialNumber) | Some(VariableTextMode::MergeCsv)
            ) {
                source.current = variable_text::advance_sequence_value(
                    source.current,
                    source.start,
                    source.end,
                    config.offset.unwrap_or(0),
                );
            }
            // Project-aware cut placeholder injection happens in the service-layer
            // text-cache refresh path before this low-level fallback is used.
            variable_text::resolve_text(&config.template, &source, 0)
        }
    } else {
        content.to_string()
    };
    if ignore_empty_vars {
        strip_unresolved_merge_fields(&resolved)
    } else {
        resolved
    }
}

fn wrap_lines(
    face: &Face<'_>,
    text: &str,
    scale_x: f64,
    letter_spacing: f64,
    max_width: Option<f64>,
    allow_wrap: bool,
) -> Vec<String> {
    let Some(max_width) = max_width.filter(|w| *w > 0.0 && w.is_finite()) else {
        return text.split('\n').map(ToString::to_string).collect();
    };
    let mut wrapped = Vec::new();
    for raw_line in text.split('\n') {
        if !allow_wrap || line_width(face, raw_line, scale_x, letter_spacing) <= max_width {
            wrapped.push(raw_line.to_string());
            continue;
        }
        let words: Vec<&str> = raw_line.split_whitespace().collect();
        if words.is_empty() {
            wrapped.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in words {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if current.is_empty()
                || line_width(face, &candidate, scale_x, letter_spacing) <= max_width
            {
                current = candidate;
            } else {
                wrapped.push(current);
                current = word.to_string();
            }
        }
        wrapped.push(current);
    }
    wrapped
}

fn wrap_lines_resolved<F>(
    text: &str,
    max_width: Option<f64>,
    allow_wrap: bool,
    mut line_width: F,
) -> Vec<String>
where
    F: FnMut(&str) -> f64,
{
    let Some(max_width) = max_width.filter(|w| *w > 0.0 && w.is_finite()) else {
        return text.split('\n').map(ToString::to_string).collect();
    };
    let mut wrapped = Vec::new();
    for raw_line in text.split('\n') {
        if !allow_wrap || line_width(raw_line) <= max_width {
            wrapped.push(raw_line.to_string());
            continue;
        }
        let words: Vec<&str> = raw_line.split_whitespace().collect();
        if words.is_empty() {
            wrapped.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in words {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if current.is_empty() || line_width(&candidate) <= max_width {
                current = candidate;
            } else {
                wrapped.push(current);
                current = word.to_string();
            }
        }
        wrapped.push(current);
    }
    wrapped
}

fn outline_resolved_glyph(
    glyph: &ResolvedGlyph,
    stretch_x: f64,
    offset_x: f64,
    baseline_y: f64,
) -> Vec<SubPath> {
    let Some(glyph_id) = glyph.glyph_id else {
        return Vec::new();
    };
    let Some(face_ref) = glyph.face else {
        return Vec::new();
    };

    match face_ref {
        ResolvedGlyphFace::System(id) => {
            let db = system_db();
            db.with_face_data(id, |data, index| {
                let face = Face::parse(data, index).ok()?;
                let scale_y = glyph.advance_mm
                    / face
                        .glyph_hor_advance(glyph_id)
                        .map(|advance| advance.max(1) as f64)
                        .unwrap_or(face.units_per_em() as f64 * 0.5);
                let scale_x = scale_y * stretch_x;
                let mut builder = GlyphPathBuilder::new(scale_x, scale_y, offset_x, baseline_y);
                face.outline_glyph(glyph_id, &mut builder)?;
                Some(builder.finish())
            })
            .flatten()
            .unwrap_or_default()
        }
        ResolvedGlyphFace::BundledFallback => {
            let Some(face) = fallback_face() else {
                return Vec::new();
            };
            let scale_y = glyph.advance_mm
                / face
                    .glyph_hor_advance(glyph_id)
                    .map(|advance| advance.max(1) as f64)
                    .unwrap_or(face.units_per_em() as f64 * 0.5);
            let scale_x = scale_y * stretch_x;
            let mut builder = GlyphPathBuilder::new(scale_x, scale_y, offset_x, baseline_y);
            if face.outline_glyph(glyph_id, &mut builder).is_some() {
                builder.finish()
            } else {
                Vec::new()
            }
        }
    }
}

fn render_text_with_system_fallback(
    db: &Database,
    primary_id: ID,
    primary_face: &Face<'_>,
    content: &str,
    font_size_mm: f64,
    alignment: TextAlignment,
    alignment_v: TextAlignmentV,
    upper_case: bool,
    h_spacing: f64,
    v_spacing: f64,
    box_w: f64,
    box_h: f64,
    max_width: Option<f64>,
    squeeze: bool,
    allow_wrap: bool,
) -> Option<(VecPath, Vec<usize>, Vec<String>)> {
    if content.trim().is_empty() || font_size_mm <= 0.0 {
        return None;
    }

    let display_text = if upper_case {
        content.to_uppercase()
    } else {
        content.to_string()
    };
    let mut fallback_cache = HashMap::<char, Option<ResolvedGlyphFace>>::new();
    let mut missing_glyphs = BTreeSet::<String>::new();
    let lines = wrap_lines_resolved(&display_text, max_width, allow_wrap, |line| {
        let glyphs = resolve_system_line_glyphs(
            db,
            primary_id,
            line,
            font_size_mm,
            &mut fallback_cache,
            &mut missing_glyphs,
        );
        resolved_line_width(&glyphs, h_spacing)
    });
    if lines.is_empty() {
        return None;
    }

    let primary_scale = font_size_mm / primary_face.units_per_em() as f64;
    let ascender = primary_face.ascender() as f64 * primary_scale;
    let descender = primary_face.descender() as f64 * primary_scale;
    let text_height = (ascender - descender).max(font_size_mm);
    let line_step = text_height + v_spacing;
    let total_text_h = text_height + (lines.len().saturating_sub(1) as f64) * line_step;
    let effective_box_w = max_width
        .filter(|w| *w > 0.0 && w.is_finite())
        .map(|w| box_w.min(w))
        .unwrap_or(box_w);

    let base_y = match alignment_v {
        TextAlignmentV::Top => ascender,
        TextAlignmentV::Middle => ((box_h - total_text_h) * 0.5).max(0.0) + ascender,
        TextAlignmentV::Bottom => (box_h - total_text_h).max(0.0) + ascender,
    };

    let mut subpaths = Vec::new();
    let mut glyph_starts: Vec<usize> = Vec::new();

    for (line_idx, line) in lines.iter().enumerate() {
        let glyphs = resolve_system_line_glyphs(
            db,
            primary_id,
            line,
            font_size_mm,
            &mut fallback_cache,
            &mut missing_glyphs,
        );
        let base_width = resolved_line_width(&glyphs, h_spacing);
        let stretch_x = if squeeze
            && let Some(max_width) = max_width.filter(|w| *w > 0.0 && w.is_finite())
            && base_width > max_width
            && base_width > f64::EPSILON
        {
            max_width / base_width
        } else {
            1.0
        };
        let line_letter_spacing = h_spacing * stretch_x;
        let width = glyphs
            .iter()
            .map(|glyph| glyph.advance_mm * stretch_x)
            .sum::<f64>()
            + if glyphs.len() > 1 {
                line_letter_spacing * (glyphs.len() - 1) as f64
            } else {
                0.0
            };
        let start_x = match alignment {
            TextAlignment::Left => 0.0,
            TextAlignment::Center => ((effective_box_w - width) * 0.5).max(0.0),
            TextAlignment::Right => (effective_box_w - width).max(0.0),
        };
        let baseline_y = base_y + line_idx as f64 * line_step;

        let mut pen_x = start_x;
        for glyph in &glyphs {
            let glyph_paths = outline_resolved_glyph(glyph, stretch_x, pen_x, baseline_y);
            if !glyph_paths.is_empty() {
                glyph_starts.push(subpaths.len());
                subpaths.extend(glyph_paths);
            }
            pen_x += glyph.advance_mm * stretch_x + line_letter_spacing;
        }
    }

    if subpaths.is_empty() && missing_glyphs.is_empty() {
        None
    } else {
        Some((
            VecPath { subpaths },
            glyph_starts,
            missing_glyphs.into_iter().collect(),
        ))
    }
}

fn render_text_with_face(
    face: &Face<'_>,
    content: &str,
    font_size_mm: f64,
    alignment: TextAlignment,
    alignment_v: TextAlignmentV,
    upper_case: bool,
    h_spacing: f64,
    v_spacing: f64,
    box_w: f64,
    box_h: f64,
    max_width: Option<f64>,
    squeeze: bool,
    allow_wrap: bool,
) -> Option<(VecPath, Vec<usize>, Vec<String>)> {
    if content.trim().is_empty() || font_size_mm <= 0.0 {
        return None;
    }

    let display_text = if upper_case {
        content.to_uppercase()
    } else {
        content.to_string()
    };
    let lines = wrap_lines(
        face,
        &display_text,
        font_size_mm / face.units_per_em() as f64,
        h_spacing,
        max_width,
        allow_wrap,
    );
    if lines.is_empty() {
        return None;
    }

    let units_per_em = face.units_per_em();
    let scale_y = font_size_mm / units_per_em as f64;
    let ascender = face.ascender() as f64 * scale_y;
    let descender = face.descender() as f64 * scale_y;
    let text_height = (ascender - descender).max(font_size_mm);
    let line_step = text_height + v_spacing;
    let total_text_h = text_height + (lines.len().saturating_sub(1) as f64) * line_step;
    let effective_box_w = max_width
        .filter(|w| *w > 0.0 && w.is_finite())
        .map(|w| box_w.min(w))
        .unwrap_or(box_w);

    let base_y = match alignment_v {
        TextAlignmentV::Top => ascender,
        TextAlignmentV::Middle => ((box_h - total_text_h) * 0.5).max(0.0) + ascender,
        TextAlignmentV::Bottom => (box_h - total_text_h).max(0.0) + ascender,
    };

    let letter_spacing = h_spacing;
    let mut subpaths = Vec::new();
    let mut glyph_starts: Vec<usize> = Vec::new();
    let mut missing_glyphs = BTreeSet::<String>::new();

    for (line_idx, line) in lines.iter().enumerate() {
        let base_width = line_width(face, line, scale_y, letter_spacing);
        let stretch_x = if squeeze
            && let Some(max_width) = max_width.filter(|w| *w > 0.0 && w.is_finite())
            && base_width > max_width
            && base_width > f64::EPSILON
        {
            max_width / base_width
        } else {
            1.0
        };
        let scale_x = scale_y * stretch_x;
        let line_letter_spacing = letter_spacing * stretch_x;
        let width = line_width(face, line, scale_x, line_letter_spacing);
        let start_x = match alignment {
            TextAlignment::Left => 0.0,
            TextAlignment::Center => ((effective_box_w - width) * 0.5).max(0.0),
            TextAlignment::Right => (effective_box_w - width).max(0.0),
        };
        let baseline_y = base_y + line_idx as f64 * line_step;

        let mut pen_x = start_x;
        for ch in line.chars() {
            let Some(glyph_id) = face.glyph_index(ch) else {
                if !ch.is_whitespace() {
                    missing_glyphs.insert(ch.to_string());
                }
                pen_x += (if ch.is_whitespace() {
                    font_size_mm * 0.33
                } else {
                    font_size_mm * 0.6
                }) + line_letter_spacing;
                continue;
            };

            let mut builder = GlyphPathBuilder::new(scale_x, scale_y, pen_x, baseline_y);
            let has_outline = face.outline_glyph(glyph_id, &mut builder).is_some();
            if has_outline {
                glyph_starts.push(subpaths.len());
                subpaths.extend(builder.finish());
            }

            pen_x += glyph_advance(face, glyph_id, scale_x, line_letter_spacing);
        }
    }

    if subpaths.is_empty() && missing_glyphs.is_empty() {
        None
    } else {
        Some((
            VecPath { subpaths },
            glyph_starts,
            missing_glyphs.into_iter().collect(),
        ))
    }
}

fn resolve_with_system_font(
    content: &str,
    font_family: &str,
    font_size_mm: f64,
    bold: bool,
    italic: bool,
    alignment: TextAlignment,
    alignment_v: TextAlignmentV,
    upper_case: bool,
    h_spacing: f64,
    v_spacing: f64,
    box_w: f64,
    box_h: f64,
    max_width: Option<f64>,
    squeeze: bool,
    allow_wrap: bool,
) -> Option<ResolvedTextPath> {
    let db = system_db();
    let generic = match font_family.to_ascii_lowercase().as_str() {
        "sans-serif" | "sans serif" => Some(Family::SansSerif),
        "serif" => Some(Family::Serif),
        "monospace" => Some(Family::Monospace),
        "cursive" => Some(Family::Cursive),
        "fantasy" => Some(Family::Fantasy),
        _ => None,
    };
    let families = if let Some(generic) = generic {
        [generic, Family::Name(font_family)]
    } else {
        [Family::Name(font_family), Family::SansSerif]
    };
    let query = Query {
        families: &families,
        weight: if bold { Weight::BOLD } else { Weight::NORMAL },
        stretch: Stretch::Normal,
        style: if italic { Style::Italic } else { Style::Normal },
    };
    let id = db.query(&query)?;
    let face_info = db.face(id)?;
    let resolved_font_key = face_info
        .families
        .first()
        .map(|(name, _)| name.clone())
        .unwrap_or_else(|| font_family.to_string());

    // Detect when fontdb matched a fallback family instead of the requested one.
    // Generic requests (sans-serif, serif, etc.) are never considered missing.
    let requested_is_generic = generic.is_some();
    let font_found_in_families = face_info
        .families
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case(font_family));
    let font_missing = !requested_is_generic && !font_found_in_families;

    db.with_face_data(id, |data, index| {
        let face = Face::parse(data, index).ok()?;
        let (path, glyph_starts, missing_glyphs) = render_text_with_system_fallback(
            db,
            id,
            &face,
            content,
            font_size_mm,
            alignment,
            alignment_v,
            upper_case,
            h_spacing,
            v_spacing,
            box_w,
            box_h,
            max_width,
            squeeze,
            allow_wrap,
        )?;
        Some(ResolvedTextPath {
            path,
            glyph_starts,
            resolved_font_source: TextFontSource::System,
            resolved_font_key: resolved_font_key.clone(),
            missing_font: font_missing,
            missing_glyphs,
        })
    })?
}

fn resolve_with_fallback_font(
    content: &str,
    font_size_mm: f64,
    alignment: TextAlignment,
    alignment_v: TextAlignmentV,
    upper_case: bool,
    h_spacing: f64,
    v_spacing: f64,
    box_w: f64,
    box_h: f64,
    max_width: Option<f64>,
    squeeze: bool,
    allow_wrap: bool,
    missing_font: bool,
) -> Option<ResolvedTextPath> {
    let face = fallback_face()?;
    let (path, glyph_starts, missing_glyphs) = render_text_with_face(
        &face,
        content,
        font_size_mm,
        alignment,
        alignment_v,
        upper_case,
        h_spacing,
        v_spacing,
        box_w,
        box_h,
        max_width,
        squeeze,
        allow_wrap,
    )?;
    Some(ResolvedTextPath {
        path,
        glyph_starts,
        resolved_font_source: TextFontSource::BundledFallback,
        resolved_font_key: "Liberation Sans".to_string(),
        missing_font,
        missing_glyphs,
    })
}

fn normalize_path_to_origin(mut path: VecPath) -> VecPath {
    let Some(bbox) = path.bounds() else {
        return path;
    };
    for subpath in &mut path.subpaths {
        for cmd in &mut subpath.commands {
            match cmd {
                PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => {
                    *x -= bbox.min.x;
                    *y -= bbox.min.y;
                }
                PathCommand::QuadTo { cx, cy, x, y } => {
                    *cx -= bbox.min.x;
                    *cy -= bbox.min.y;
                    *x -= bbox.min.x;
                    *y -= bbox.min.y;
                }
                PathCommand::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                } => {
                    *c1x -= bbox.min.x;
                    *c1y -= bbox.min.y;
                    *c2x -= bbox.min.x;
                    *c2y -= bbox.min.y;
                    *x -= bbox.min.x;
                    *y -= bbox.min.y;
                }
                PathCommand::Close => {}
            }
        }
    }
    path
}

const MAX_TEXT_TRANSFORM_COORDINATE_MM: f64 = 1_000_000.0;

fn normalized_transform_curve(curve: f64) -> f64 {
    if curve.is_finite() {
        curve.clamp(-100.0, 100.0)
    } else {
        0.0
    }
}

fn finite_transform_coordinate(value: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value.clamp(
            -MAX_TEXT_TRANSFORM_COORDINATE_MM,
            MAX_TEXT_TRANSFORM_COORDINATE_MM,
        )
    } else if fallback.is_finite() {
        fallback.clamp(
            -MAX_TEXT_TRANSFORM_COORDINATE_MM,
            MAX_TEXT_TRANSFORM_COORDINATE_MM,
        )
    } else {
        0.0
    }
}

fn map_path_points(mut path: VecPath, mut map: impl FnMut(f64, f64) -> (f64, f64)) -> VecPath {
    let mut apply = |x: &mut f64, y: &mut f64| {
        let original = (*x, *y);
        let (mapped_x, mapped_y) = map(original.0, original.1);
        *x = finite_transform_coordinate(mapped_x, original.0);
        *y = finite_transform_coordinate(mapped_y, original.1);
    };

    for subpath in &mut path.subpaths {
        for command in &mut subpath.commands {
            match command {
                PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => apply(x, y),
                PathCommand::QuadTo { cx, cy, x, y } => {
                    apply(cx, cy);
                    apply(x, y);
                }
                PathCommand::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                } => {
                    apply(c1x, c1y);
                    apply(c2x, c2y);
                    apply(x, y);
                }
                PathCommand::Close => {}
            }
        }
    }
    path
}

/// Apply the shared envelope mapper used by the editable Rise/Wave/Flag/Angle styles.
/// The exact zero strength path is returned unchanged for a stable identity transform.
fn apply_text_envelope_transform(path: VecPath, style: TextTransformStyle, curve: f64) -> VecPath {
    let curve = normalized_transform_curve(curve);
    if curve.abs() <= f64::EPSILON {
        return path;
    }
    let Some(bounds) = path.bounds() else {
        return path;
    };
    let width = bounds.width();
    let height = bounds.height();
    if !width.is_finite() || !height.is_finite() || width <= f64::EPSILON || height <= f64::EPSILON
    {
        return path;
    }

    let amplitude = (curve / 100.0 * height * 1.5).clamp(
        -MAX_TEXT_TRANSFORM_COORDINATE_MM / 4.0,
        MAX_TEXT_TRANSFORM_COORDINATE_MM / 4.0,
    );
    map_path_points(path, |x, y| {
        let u = ((x - bounds.min.x) / width).clamp(0.0, 1.0);
        let v = ((y - bounds.min.y) / height).clamp(0.0, 1.0);
        let wave = (std::f64::consts::TAU * u).sin();
        let offset_y = match style {
            // Move every horizontal slice by the same ramp, preserving glyph shapes.
            TextTransformStyle::Rise => -amplitude * (2.0 * u - 1.0),
            // Opposing top/bottom envelopes create the expanding/contracting wave.
            TextTransformStyle::Wave => amplitude * wave * (1.0 - 2.0 * v),
            // A flag moves the full vertical slice together along a sine baseline.
            TextTransformStyle::Flag => amplitude * wave,
            // Keep the lower edge anchored while sloping the upper envelope.
            TextTransformStyle::Angle => -amplitude * (2.0 * u - 1.0) * (1.0 - v),
            _ => 0.0,
        };
        (x, y + offset_y)
    })
}

/// Return the font ascender in mm for a given font configuration.
/// The ascender is the distance from baseline to the top of the tallest glyphs.
/// Falls back to `font_size_mm * 0.8` if the font cannot be resolved.
pub fn font_ascender_mm(font_family: &str, font_size_mm: f64, bold: bool, italic: bool) -> f64 {
    let db = system_db();
    let generic = match font_family.to_ascii_lowercase().as_str() {
        "sans-serif" | "sans serif" => Some(Family::SansSerif),
        "serif" => Some(Family::Serif),
        "monospace" => Some(Family::Monospace),
        "cursive" => Some(Family::Cursive),
        "fantasy" => Some(Family::Fantasy),
        _ => None,
    };
    let families = if let Some(generic) = generic {
        [generic, Family::Name(font_family)]
    } else {
        [Family::Name(font_family), Family::SansSerif]
    };
    let query = Query {
        families: &families,
        weight: if bold { Weight::BOLD } else { Weight::NORMAL },
        stretch: Stretch::Normal,
        style: if italic { Style::Italic } else { Style::Normal },
    };
    if let Some(id) = db.query(&query) {
        let result = db.with_face_data(id, |data, index| {
            if let Ok(face) = Face::parse(data, index) {
                let scale = font_size_mm / face.units_per_em() as f64;
                face.ascender() as f64 * scale
            } else {
                font_size_mm * 0.8
            }
        });
        result.unwrap_or(font_size_mm * 0.8)
    } else if let Some(face) = fallback_face() {
        let scale = font_size_mm / face.units_per_em() as f64;
        face.ascender() as f64 * scale
    } else {
        font_size_mm * 0.8
    }
}

pub fn available_font_families() -> Vec<String> {
    let mut families = BTreeSet::new();
    for face in system_db().faces() {
        if let Some((name, _)) = face.families.first() {
            // Skip dot-prefixed internal system fonts (e.g. .AppleSystemUIFont, .SFNSMono)
            if !name.starts_with('.') {
                families.insert(name.clone());
            }
        }
    }
    families.into_iter().collect()
}

pub(crate) fn resolve_text_in_box_with_options(
    content: &str,
    font_family: &str,
    font_size_mm: f64,
    bold: bool,
    italic: bool,
    alignment: TextAlignment,
    alignment_v: TextAlignmentV,
    upper_case: bool,
    h_spacing: f64,
    v_spacing: f64,
    box_w: f64,
    box_h: f64,
    max_width: Option<f64>,
    squeeze: bool,
    allow_wrap: bool,
) -> Option<ResolvedTextPath> {
    let missing_fallback_font = !is_generic_font_family(font_family);
    resolve_with_system_font(
        content,
        font_family,
        font_size_mm,
        bold,
        italic,
        alignment,
        alignment_v,
        upper_case,
        h_spacing,
        v_spacing,
        box_w,
        box_h,
        max_width,
        squeeze,
        allow_wrap,
    )
    .or_else(|| {
        resolve_with_fallback_font(
            content,
            font_size_mm,
            alignment,
            alignment_v,
            upper_case,
            h_spacing,
            v_spacing,
            box_w,
            box_h,
            max_width,
            squeeze,
            allow_wrap,
            missing_fallback_font,
        )
    })
}

pub fn resolve_text_in_box(
    content: &str,
    font_family: &str,
    font_size_mm: f64,
    bold: bool,
    italic: bool,
    alignment: TextAlignment,
    alignment_v: TextAlignmentV,
    upper_case: bool,
    h_spacing: f64,
    v_spacing: f64,
    box_w: f64,
    box_h: f64,
) -> Option<ResolvedTextPath> {
    resolve_text_in_box_with_options(
        content,
        font_family,
        font_size_mm,
        bold,
        italic,
        alignment,
        alignment_v,
        upper_case,
        h_spacing,
        v_spacing,
        box_w,
        box_h,
        None,
        false,
        true,
    )
}

/// Convert text content to a VecPath using actual glyph outlines.
pub fn text_to_vecpath(
    content: &str,
    font_family: &str,
    font_size_mm: f64,
    bold: bool,
    italic: bool,
) -> Option<VecPath> {
    let resolved = resolve_text_in_box(
        content,
        font_family,
        font_size_mm,
        bold,
        italic,
        TextAlignment::Left,
        TextAlignmentV::Top,
        false,
        0.0,
        0.0,
        f64::MAX / 4.0,
        f64::MAX / 4.0,
    )?;
    Some(normalize_path_to_origin(resolved.path))
}

pub fn refresh_text_object_cache(data: &mut ObjectData, bounds: &Bounds) -> Option<Bounds> {
    refresh_text_object_cache_with_guide(data, bounds, None)
}

pub fn refresh_text_object_cache_with_guide(
    data: &mut ObjectData,
    bounds: &Bounds,
    guide_path: Option<&VecPath>,
) -> Option<Bounds> {
    let ObjectData::Text {
        content,
        font_family,
        font_size_mm,
        alignment,
        alignment_v,
        bold,
        italic,
        upper_case,
        welded,
        h_spacing,
        v_spacing,
        on_path,
        path_offset,
        distort,
        layout_mode,
        rtl,
        bend_radius,
        transform_style,
        transform_curve,
        circle_placement,
        max_width,
        squeeze,
        ignore_empty_vars,
        resolved_font_source,
        resolved_font_key,
        resolved_path_data,
        missing_font,
        missing_glyphs,
        variable_text,
        ..
    } = data
    else {
        return None;
    };

    let resolved_content =
        resolve_text_content_for_object(content, variable_text.as_ref(), *ignore_empty_vars);

    // Editable transformations take precedence over both modern and legacy layout modes.
    // A zero-strength non-Circle transform deliberately resolves as straight text rather
    // than falling through to Bend/Path.
    if *transform_style != TextTransformStyle::None {
        let curve = normalized_transform_curve(*transform_curve);

        let transformed = match *transform_style {
            TextTransformStyle::Arch if curve.abs() > f64::EPSILON => {
                use crate::text_path::{
                    generate_arc_guide, layout_text_on_path_resolved_with_options,
                };
                let intrinsic = resolve_text_in_box_with_options(
                    &resolved_content,
                    font_family,
                    *font_size_mm,
                    *bold,
                    *italic,
                    TextAlignment::Left,
                    TextAlignmentV::Top,
                    *upper_case,
                    *h_spacing,
                    0.0,
                    f64::MAX / 4.0,
                    f64::MAX / 4.0,
                    *max_width,
                    *squeeze,
                    false,
                );
                intrinsic
                    .as_ref()
                    .and_then(|resolved| resolved.path.bounds())
                    .and_then(|intrinsic_bounds| {
                        let width = intrinsic_bounds.width();
                        if !width.is_finite() || width <= f64::EPSILON {
                            return None;
                        }
                        let sweep = std::f64::consts::PI * curve / 100.0;
                        let radius = (width / sweep.abs())
                            .clamp((*font_size_mm).abs().max(0.01) * 0.5, 1_000_000.0)
                            * sweep.signum();
                        let guide = generate_arc_guide(width, radius);
                        layout_text_on_path_resolved_with_options(
                            &resolved_content,
                            font_family,
                            *font_size_mm,
                            *bold,
                            *italic,
                            *upper_case,
                            *h_spacing,
                            0.0,
                            *rtl,
                            *welded,
                            *distort,
                            *max_width,
                            *squeeze,
                            &guide,
                        )
                        .map(|result| ResolvedTextPath {
                            path: result.path,
                            glyph_starts: Vec::new(),
                            resolved_font_source: result.resolved_font_source,
                            resolved_font_key: result.resolved_font_key,
                            missing_font: result.missing_font,
                            missing_glyphs: result.missing_glyphs,
                        })
                    })
            }
            TextTransformStyle::Circle => {
                use crate::text_path::{
                    circle_guide_path_offset, generate_circle_guide,
                    layout_text_on_path_resolved_with_options_and_normal_offset,
                };
                let intrinsic = resolve_text_in_box_with_options(
                    &resolved_content,
                    font_family,
                    *font_size_mm,
                    *bold,
                    *italic,
                    TextAlignment::Left,
                    TextAlignmentV::Top,
                    *upper_case,
                    *h_spacing,
                    0.0,
                    f64::MAX / 4.0,
                    f64::MAX / 4.0,
                    *max_width,
                    *squeeze,
                    false,
                );
                intrinsic
                    .as_ref()
                    .and_then(|resolved| resolved.path.bounds())
                    .and_then(|intrinsic_bounds| {
                        let width = intrinsic_bounds.width();
                        if !width.is_finite() || width <= f64::EPSILON {
                            return None;
                        }
                        let guide =
                            generate_circle_guide(width, *font_size_mm, curve, *circle_placement);
                        let centered_path_offset =
                            circle_guide_path_offset(width, *font_size_mm, curve);
                        let normal_offset = match *circle_placement {
                            TextCirclePlacement::TopInside | TextCirclePlacement::BottomOutside => {
                                (*font_size_mm).abs().max(0.01)
                            }
                            TextCirclePlacement::TopOutside | TextCirclePlacement::BottomInside => {
                                0.0
                            }
                        };
                        layout_text_on_path_resolved_with_options_and_normal_offset(
                            &resolved_content,
                            font_family,
                            *font_size_mm,
                            *bold,
                            *italic,
                            *upper_case,
                            *h_spacing,
                            centered_path_offset,
                            *rtl,
                            *welded,
                            *distort,
                            *max_width,
                            *squeeze,
                            normal_offset,
                            &guide,
                        )
                        .map(|result| ResolvedTextPath {
                            path: result.path,
                            glyph_starts: Vec::new(),
                            resolved_font_source: result.resolved_font_source,
                            resolved_font_key: result.resolved_font_key,
                            missing_font: result.missing_font,
                            missing_glyphs: result.missing_glyphs,
                        })
                    })
            }
            style => resolve_text_in_box_with_options(
                &resolved_content,
                font_family,
                *font_size_mm,
                *bold,
                *italic,
                TextAlignment::Left,
                TextAlignmentV::Top,
                *upper_case,
                *h_spacing,
                *v_spacing,
                f64::MAX / 4.0,
                f64::MAX / 4.0,
                *max_width,
                *squeeze,
                false,
            )
            .map(|mut resolved| {
                resolved.path = apply_text_envelope_transform(resolved.path, style, curve);
                resolved
            }),
        };

        match transformed {
            Some(mut resolved) => {
                // Path-layout transforms can receive hostile persisted dimensions;
                // clamp every endpoint/control point before deriving bounds or SVG.
                resolved.path = map_path_points(resolved.path, |x, y| (x, y));
                let mapped_bounds = resolved.path.bounds();
                *resolved_font_source = Some(resolved.resolved_font_source);
                *resolved_font_key = Some(resolved.resolved_font_key);
                *resolved_path_data = Some(normalize_path_to_origin(resolved.path).to_svg_d());
                *missing_font = resolved.missing_font;
                *missing_glyphs = resolved.missing_glyphs;
                return mapped_bounds;
            }
            None => {
                *resolved_path_data = None;
                *resolved_font_source = None;
                *resolved_font_key = None;
                *missing_font = false;
                missing_glyphs.clear();
                return None;
            }
        }
    }

    let effective_layout = if *on_path && *layout_mode == TextLayoutMode::Straight {
        TextLayoutMode::Path
    } else {
        *layout_mode
    };

    // Path mode: lay text along guide path
    if effective_layout == TextLayoutMode::Path {
        if let Some(guide) = guide_path {
            use crate::text_path::layout_text_on_path_resolved_with_options;
            if let Some(result) = layout_text_on_path_resolved_with_options(
                &resolved_content,
                font_family,
                *font_size_mm,
                *bold,
                *italic,
                *upper_case,
                *h_spacing,
                *path_offset,
                *rtl,
                *welded,
                *distort,
                *max_width,
                *squeeze,
                guide,
            ) {
                let mapped_bounds = result.path.bounds();
                *resolved_path_data = Some(normalize_path_to_origin(result.path).to_svg_d());
                *resolved_font_source = Some(result.resolved_font_source);
                *resolved_font_key = Some(result.resolved_font_key);
                *missing_font = result.missing_font;
                *missing_glyphs = result.missing_glyphs;
                return mapped_bounds;
            }
        }
        // No guide or layout failed — clear cache
        *resolved_path_data = None;
        *resolved_font_source = None;
        *resolved_font_key = None;
        *missing_font = false;
        missing_glyphs.clear();
        return None;
    }

    // Bend mode: generate arc guide from bend_radius and lay text along it.
    if effective_layout == TextLayoutMode::Bend && bend_radius.abs() > f64::EPSILON {
        use crate::text_path::{generate_arc_guide, layout_text_on_path_resolved_with_options};
        // Resolve straight text first to measure intrinsic width
        let intrinsic = resolve_text_in_box_with_options(
            &resolved_content,
            font_family,
            *font_size_mm,
            *bold,
            *italic,
            TextAlignment::Left,
            TextAlignmentV::Top,
            *upper_case,
            *h_spacing,
            0.0,
            f64::MAX / 4.0,
            f64::MAX / 4.0,
            *max_width,
            *squeeze,
            false,
        );
        let text_width = intrinsic
            .as_ref()
            .and_then(|r| r.path.bounds())
            .map(|b| b.width())
            .unwrap_or(0.0);
        if text_width > f64::EPSILON {
            let arc_guide = generate_arc_guide(text_width, *bend_radius);
            if let Some(result) = layout_text_on_path_resolved_with_options(
                &resolved_content,
                font_family,
                *font_size_mm,
                *bold,
                *italic,
                *upper_case,
                *h_spacing,
                *path_offset,
                *rtl,
                *welded,
                *distort,
                *max_width,
                *squeeze,
                &arc_guide,
            ) {
                let mapped_bounds = result.path.bounds();
                *resolved_path_data = Some(normalize_path_to_origin(result.path).to_svg_d());
                *resolved_font_source = Some(result.resolved_font_source);
                *resolved_font_key = Some(result.resolved_font_key);
                *missing_font = result.missing_font;
                *missing_glyphs = result.missing_glyphs;
                return mapped_bounds;
            }
        }
        // text_width ≈ 0 or layout failed — fall through to straight
    }

    // Any non-Path/non-Bend mode falls through to straight-text rendering
    // as a safe, visible fallback. Also handles bend_radius ≈ 0.

    let box_w = bounds.width().max(0.0);
    let box_h = bounds.height().max(0.0);
    let resolved = resolve_text_in_box_with_options(
        &resolved_content,
        font_family,
        *font_size_mm,
        *bold,
        *italic,
        *alignment,
        *alignment_v,
        *upper_case,
        *h_spacing,
        *v_spacing,
        box_w,
        box_h,
        *max_width,
        *squeeze,
        true,
    );

    match resolved {
        Some(resolved) => {
            *resolved_font_source = Some(resolved.resolved_font_source);
            *resolved_font_key = Some(resolved.resolved_font_key);
            *resolved_path_data = Some(normalize_path_to_origin(resolved.path).to_svg_d());
            *missing_font = resolved.missing_font;
            *missing_glyphs = resolved.missing_glyphs;
        }
        None => {
            *resolved_path_data = None;
            *resolved_font_source = None;
            *resolved_font_key = None;
            *missing_font = false;
            missing_glyphs.clear();
        }
    }
    None
}

pub fn text_object_local_path(data: &ObjectData) -> Option<VecPath> {
    match data {
        ObjectData::Text {
            resolved_path_data,
            content,
            font_family,
            font_size_mm,
            bold,
            italic,
            ..
        } => {
            if let Some(path_data) = resolved_path_data {
                Some(VecPath::parse_svg_d(path_data))
            } else {
                text_to_vecpath(content, font_family, *font_size_mm, *bold, *italic)
            }
        }
        _ => None,
    }
}

pub fn text_object_local_path_with_bounds(data: &ObjectData, bounds: &Bounds) -> Option<VecPath> {
    match data {
        ObjectData::Text {
            content,
            font_family,
            font_size_mm,
            alignment,
            alignment_v,
            bold,
            italic,
            upper_case,
            h_spacing,
            v_spacing,
            on_path,
            layout_mode,
            transform_style,
            max_width,
            squeeze,
            ignore_empty_vars,
            variable_text,
            resolved_path_data,
            ..
        } => {
            if let Some(path_data) = resolved_path_data {
                return Some(VecPath::parse_svg_d(path_data));
            }
            let effective_layout = if *on_path && *layout_mode == TextLayoutMode::Straight {
                TextLayoutMode::Path
            } else {
                *layout_mode
            };
            if *transform_style != TextTransformStyle::None
                || effective_layout != TextLayoutMode::Straight
            {
                return None;
            }
            let resolved_content = resolve_text_content_for_object(
                content,
                variable_text.as_ref(),
                *ignore_empty_vars,
            );
            resolve_text_in_box_with_options(
                &resolved_content,
                font_family,
                *font_size_mm,
                *bold,
                *italic,
                *alignment,
                *alignment_v,
                *upper_case,
                *h_spacing,
                *v_spacing,
                bounds.width().max(0.0),
                bounds.height().max(0.0),
                *max_width,
                *squeeze,
                true,
            )
            .map(|resolved| resolved.path)
        }
        _ => None,
    }
}

pub fn intrinsic_text_local_path(data: &ObjectData) -> Option<VecPath> {
    match data {
        ObjectData::Text {
            content,
            font_family,
            font_size_mm,
            bold,
            italic,
            upper_case,
            h_spacing,
            v_spacing,
            on_path,
            layout_mode,
            transform_style,
            max_width,
            squeeze,
            ignore_empty_vars,
            variable_text,
            ..
        } => {
            let effective_layout = if *on_path && *layout_mode == TextLayoutMode::Straight {
                TextLayoutMode::Path
            } else {
                *layout_mode
            };
            if *transform_style != TextTransformStyle::None
                || effective_layout != TextLayoutMode::Straight
            {
                return None;
            }
            let resolved_content = resolve_text_content_for_object(
                content,
                variable_text.as_ref(),
                *ignore_empty_vars,
            );
            resolve_text_in_box_with_options(
                &resolved_content,
                font_family,
                *font_size_mm,
                *bold,
                *italic,
                TextAlignment::Left,
                TextAlignmentV::Top,
                *upper_case,
                *h_spacing,
                *v_spacing,
                f64::MAX / 4.0,
                f64::MAX / 4.0,
                *max_width,
                *squeeze,
                true,
            )
            .map(|resolved| resolved.path)
        }
        _ => None,
    }
}

pub fn intrinsic_text_bounds(data: &ObjectData) -> Option<Bounds> {
    intrinsic_text_local_path(data).and_then(|path| path.bounds())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::ObjectData;
    use crate::variable_text::{VariableTextConfig, VariableTextSource};
    use beambench_common::geometry::Point2D;
    use std::collections::HashMap;

    fn system_font_supporting(ch: char) -> Option<String> {
        let db = system_db();
        db.faces().find_map(|face_info| {
            system_face_glyph(db, face_info.id, ch, 10.0, !ch.is_whitespace())?;
            face_info.families.first().map(|(name, _)| name.clone())
        })
    }

    fn system_font_lacking(ch: char) -> Option<String> {
        let db = system_db();
        db.faces().find_map(|face_info| {
            if system_face_glyph(db, face_info.id, 'A', 10.0, true).is_none() {
                return None;
            }
            if system_face_glyph(db, face_info.id, ch, 10.0, !ch.is_whitespace()).is_some() {
                return None;
            }
            face_info.families.first().map(|(name, _)| name.clone())
        })
    }

    fn text_data(content: &str, font_family: &str) -> ObjectData {
        ObjectData::Text {
            content: content.to_string(),
            font_family: font_family.to_string(),
            font_size_mm: 10.0,
            alignment: TextAlignment::Left,
            alignment_v: TextAlignmentV::Top,
            bold: false,
            italic: false,
            upper_case: false,
            welded: false,
            h_spacing: 0.0,
            v_spacing: 0.0,
            on_path: false,
            path_offset: 0.0,
            distort: false,
            layout_mode: TextLayoutMode::Straight,
            rtl: false,
            bend_radius: 0.0,
            transform_style: crate::object::TextTransformStyle::None,
            transform_curve: 0.0,
            circle_placement: crate::object::TextCirclePlacement::TopOutside,
            resolved_font_source: None,
            resolved_font_key: None,
            resolved_path_data: None,
            missing_font: false,
            missing_glyphs: Vec::new(),
            guide_path_id: None,
            variable_text: None,
            max_width: None,
            squeeze: false,
            ignore_empty_vars: false,
        }
    }

    fn set_transform(
        data: &mut ObjectData,
        style: TextTransformStyle,
        curve: f64,
        placement: TextCirclePlacement,
    ) {
        let ObjectData::Text {
            transform_style,
            transform_curve,
            circle_placement,
            ..
        } = data
        else {
            panic!("expected text")
        };
        *transform_style = style;
        *transform_curve = curve;
        *circle_placement = placement;
    }

    fn cached_text_path(data: &ObjectData) -> VecPath {
        let ObjectData::Text {
            resolved_path_data: Some(path_data),
            ..
        } = data
        else {
            panic!("expected cached text geometry")
        };
        VecPath::parse_svg_d(path_data)
    }

    fn assert_finite_path(path: &VecPath) {
        for subpath in &path.subpaths {
            for command in &subpath.commands {
                let finite = match command {
                    PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => {
                        x.is_finite() && y.is_finite()
                    }
                    PathCommand::QuadTo { cx, cy, x, y } => {
                        cx.is_finite() && cy.is_finite() && x.is_finite() && y.is_finite()
                    }
                    PathCommand::CubicTo {
                        c1x,
                        c1y,
                        c2x,
                        c2y,
                        x,
                        y,
                    } => {
                        c1x.is_finite()
                            && c1y.is_finite()
                            && c2x.is_finite()
                            && c2y.is_finite()
                            && x.is_finite()
                            && y.is_finite()
                    }
                    PathCommand::Close => true,
                };
                assert!(finite, "transformed path contains non-finite geometry");
            }
        }
        let bounds = path.bounds().expect("transformed path should have bounds");
        assert!(bounds.width().is_finite());
        assert!(bounds.height().is_finite());
        assert!(bounds.width() <= MAX_TEXT_TRANSFORM_COORDINATE_MM * 2.0);
        assert!(bounds.height() <= MAX_TEXT_TRANSFORM_COORDINATE_MM * 2.0);
    }

    #[test]
    fn font_ascender_mm_returns_reasonable_value() {
        let ascender = font_ascender_mm("Arial", 10.0, false, false);
        // Ascender should be roughly 70-95% of font_size for typical fonts
        assert!(
            ascender > 5.0,
            "ascender should be > 50% of font_size: {ascender}"
        );
        assert!(
            ascender < 10.0,
            "ascender should be < 100% of font_size: {ascender}"
        );
    }

    #[test]
    fn font_ascender_mm_scales_with_font_size() {
        let small = font_ascender_mm("Arial", 5.0, false, false);
        let large = font_ascender_mm("Arial", 20.0, false, false);
        assert!(large > small * 3.0, "ascender should scale with font_size");
    }

    #[test]
    fn text_to_vecpath_produces_path() {
        let result = text_to_vecpath("Hello", "Arial", 10.0, false, false);
        assert!(result.is_some());
        assert!(!result.unwrap().is_empty());
    }

    #[test]
    fn empty_text_returns_none() {
        assert!(text_to_vecpath("", "Arial", 10.0, false, false).is_none());
    }

    #[test]
    fn font_size_affects_bounds() {
        let small = text_to_vecpath("A", "Arial", 5.0, false, false).unwrap();
        let large = text_to_vecpath("A", "Arial", 20.0, false, false).unwrap();
        let small_b = small.bounds().unwrap();
        let large_b = large.bounds().unwrap();
        assert!(large_b.width() > small_b.width());
        assert!(large_b.height() > small_b.height());
    }

    #[test]
    fn refresh_text_object_cache_embeds_resolved_path() {
        let mut data = ObjectData::Text {
            content: "Hello".to_string(),
            font_family: "Arial".to_string(),
            font_size_mm: 10.0,
            alignment: TextAlignment::Left,
            alignment_v: TextAlignmentV::Top,
            bold: false,
            italic: false,
            upper_case: false,
            welded: false,
            h_spacing: 0.0,
            v_spacing: 0.0,
            on_path: false,
            path_offset: 0.0,
            distort: false,
            layout_mode: TextLayoutMode::Straight,
            rtl: false,
            bend_radius: 0.0,
            transform_style: crate::object::TextTransformStyle::None,
            transform_curve: 0.0,
            circle_placement: crate::object::TextCirclePlacement::TopOutside,
            resolved_font_source: None,
            resolved_font_key: None,
            resolved_path_data: None,
            missing_font: false,
            missing_glyphs: Vec::new(),
            guide_path_id: None,
            variable_text: None,
            max_width: None,
            squeeze: false,
            ignore_empty_vars: false,
        };
        refresh_text_object_cache(
            &mut data,
            &Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(40.0, 10.0)),
        );
        match data {
            ObjectData::Text {
                resolved_path_data,
                resolved_font_key,
                ..
            } => {
                assert!(resolved_path_data.is_some());
                assert!(resolved_font_key.is_some());
            }
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn refresh_text_object_cache_normalizes_path_to_local_origin() {
        let mut data = ObjectData::Text {
            content: "Hello".to_string(),
            font_family: "Arial".to_string(),
            font_size_mm: 10.0,
            alignment: TextAlignment::Left,
            alignment_v: TextAlignmentV::Top,
            bold: false,
            italic: false,
            upper_case: false,
            welded: false,
            h_spacing: 0.0,
            v_spacing: 0.0,
            on_path: false,
            path_offset: 0.0,
            distort: false,
            layout_mode: TextLayoutMode::Straight,
            rtl: false,
            bend_radius: 0.0,
            transform_style: crate::object::TextTransformStyle::None,
            transform_curve: 0.0,
            circle_placement: crate::object::TextCirclePlacement::TopOutside,
            resolved_font_source: None,
            resolved_font_key: None,
            resolved_path_data: None,
            missing_font: false,
            missing_glyphs: Vec::new(),
            guide_path_id: None,
            variable_text: None,
            max_width: None,
            squeeze: false,
            ignore_empty_vars: false,
        };
        refresh_text_object_cache(
            &mut data,
            &Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(200.0, 80.0)),
        );
        match data {
            ObjectData::Text {
                resolved_path_data: Some(path_data),
                ..
            } => {
                let path = VecPath::parse_svg_d(&path_data);
                let bbox = path.bounds().unwrap();
                assert!(bbox.min.x.abs() < 1e-6);
                assert!(bbox.min.y.abs() < 1e-6);
            }
            _ => panic!("expected resolved text path"),
        }
    }

    #[test]
    fn system_font_list_is_not_empty() {
        assert!(!available_font_families().is_empty());
    }

    #[test]
    fn bend_mode_zero_radius_falls_back_to_straight() {
        let mut data = ObjectData::Text {
            content: "Hello".to_string(),
            font_family: "Arial".to_string(),
            font_size_mm: 10.0,
            alignment: TextAlignment::Left,
            alignment_v: TextAlignmentV::Top,
            bold: false,
            italic: false,
            upper_case: false,
            welded: false,
            h_spacing: 0.0,
            v_spacing: 0.0,
            on_path: false,
            path_offset: 0.0,
            distort: false,
            layout_mode: TextLayoutMode::Bend,
            rtl: false,
            bend_radius: 0.0,
            transform_style: crate::object::TextTransformStyle::None,
            transform_curve: 0.0,
            circle_placement: crate::object::TextCirclePlacement::TopOutside,
            resolved_font_source: None,
            resolved_font_key: None,
            resolved_path_data: None,
            missing_font: false,
            missing_glyphs: Vec::new(),
            guide_path_id: None,
            variable_text: None,
            max_width: None,
            squeeze: false,
            ignore_empty_vars: false,
        };
        refresh_text_object_cache(
            &mut data,
            &Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(40.0, 10.0)),
        );
        match data {
            ObjectData::Text {
                resolved_path_data, ..
            } => {
                assert!(
                    resolved_path_data.is_some(),
                    "Bend mode with zero radius should fall back to straight rendering"
                );
            }
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn zero_curve_transform_is_straight_identity_and_overrides_layout_mode() {
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 30.0));
        let mut straight = text_data("Identity", "Liberation Sans");
        refresh_text_object_cache(&mut straight, &bounds);
        let expected = cached_text_path(&straight).to_svg_d();

        for style in [
            TextTransformStyle::Arch,
            TextTransformStyle::Rise,
            TextTransformStyle::Wave,
            TextTransformStyle::Flag,
            TextTransformStyle::Angle,
        ] {
            let mut transformed = text_data("Identity", "Liberation Sans");
            if let ObjectData::Text { layout_mode, .. } = &mut transformed {
                *layout_mode = TextLayoutMode::Bend;
            }
            set_transform(
                &mut transformed,
                style,
                0.0,
                TextCirclePlacement::TopOutside,
            );
            refresh_text_object_cache(&mut transformed, &bounds);
            assert_eq!(
                cached_text_path(&transformed).to_svg_d(),
                expected,
                "{style:?}"
            );
        }
    }

    #[test]
    fn envelope_transform_styles_generate_distinct_finite_geometry() {
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(120.0, 35.0));
        let mut paths = BTreeSet::new();
        for style in [
            TextTransformStyle::Rise,
            TextTransformStyle::Wave,
            TextTransformStyle::Flag,
            TextTransformStyle::Angle,
        ] {
            let mut data = text_data("Transform", "Liberation Sans");
            set_transform(&mut data, style, 75.0, TextCirclePlacement::TopOutside);
            refresh_text_object_cache(&mut data, &bounds);
            let path = cached_text_path(&data);
            assert_finite_path(&path);
            paths.insert(path.to_svg_d());
        }
        assert_eq!(
            paths.len(),
            4,
            "envelope styles should not collapse to one shape"
        );
    }

    #[test]
    fn transform_curve_is_clamped_and_non_finite_values_are_safe() {
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 30.0));
        let mut clamped = text_data("Safe", "Liberation Sans");
        set_transform(
            &mut clamped,
            TextTransformStyle::Flag,
            100.0,
            TextCirclePlacement::TopOutside,
        );
        refresh_text_object_cache(&mut clamped, &bounds);

        let mut oversized = text_data("Safe", "Liberation Sans");
        set_transform(
            &mut oversized,
            TextTransformStyle::Flag,
            1.0e300,
            TextCirclePlacement::TopOutside,
        );
        refresh_text_object_cache(&mut oversized, &bounds);
        assert_eq!(
            cached_text_path(&oversized).to_svg_d(),
            cached_text_path(&clamped).to_svg_d()
        );

        let mut non_finite = text_data("Safe", "Liberation Sans");
        set_transform(
            &mut non_finite,
            TextTransformStyle::Flag,
            f64::NAN,
            TextCirclePlacement::TopOutside,
        );
        refresh_text_object_cache(&mut non_finite, &bounds);
        assert_finite_path(&cached_text_path(&non_finite));
    }

    #[test]
    fn arch_and_all_circle_placements_generate_distinct_finite_geometry() {
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(140.0, 60.0));
        let mut arch = text_data("Readable", "Liberation Sans");
        set_transform(
            &mut arch,
            TextTransformStyle::Arch,
            80.0,
            TextCirclePlacement::TopOutside,
        );
        refresh_text_object_cache(&mut arch, &bounds);
        assert_finite_path(&cached_text_path(&arch));

        let mut circle_paths = BTreeSet::new();
        let mut circle_bounds = Vec::new();
        for placement in [
            TextCirclePlacement::TopOutside,
            TextCirclePlacement::TopInside,
            TextCirclePlacement::BottomOutside,
            TextCirclePlacement::BottomInside,
        ] {
            let mut data = text_data("ABC", "Liberation Sans");
            set_transform(&mut data, TextTransformStyle::Circle, 35.0, placement);
            refresh_text_object_cache(&mut data, &bounds);
            let path = cached_text_path(&data);
            assert_finite_path(&path);
            circle_bounds.push((placement, path.bounds().unwrap()));
            circle_paths.insert(path.to_svg_d());
        }
        assert_eq!(
            circle_paths.len(),
            4,
            "normalized Circle output must preserve all four placement geometries"
        );
        let bounds_for = |placement| {
            circle_bounds
                .iter()
                .find(|(candidate, _)| *candidate == placement)
                .map(|(_, bounds)| *bounds)
                .unwrap()
        };
        let top_outside = bounds_for(TextCirclePlacement::TopOutside);
        let top_inside = bounds_for(TextCirclePlacement::TopInside);
        let bottom_outside = bounds_for(TextCirclePlacement::BottomOutside);
        let bottom_inside = bounds_for(TextCirclePlacement::BottomInside);
        assert!(
            (top_outside.width() - top_inside.width()).abs() > 1e-6
                || (top_outside.height() - top_inside.height()).abs() > 1e-6,
            "top inside/outside placements need distinct normalized bounds"
        );
        assert!(
            (bottom_outside.width() - bottom_inside.width()).abs() > 1e-6
                || (bottom_outside.height() - bottom_inside.height()).abs() > 1e-6,
            "bottom inside/outside placements need distinct normalized bounds"
        );
    }

    fn make_bend_text(bend_radius: f64) -> ObjectData {
        ObjectData::Text {
            content: "Hello".to_string(),
            font_family: "Arial".to_string(),
            font_size_mm: 10.0,
            alignment: TextAlignment::Left,
            alignment_v: TextAlignmentV::Top,
            bold: false,
            italic: false,
            upper_case: false,
            welded: false,
            h_spacing: 0.0,
            v_spacing: 0.0,
            on_path: false,
            path_offset: 0.0,
            distort: false,
            layout_mode: TextLayoutMode::Bend,
            rtl: false,
            bend_radius,
            transform_style: TextTransformStyle::None,
            transform_curve: 0.0,
            circle_placement: TextCirclePlacement::TopOutside,
            resolved_font_source: None,
            resolved_font_key: None,
            resolved_path_data: None,
            missing_font: false,
            missing_glyphs: Vec::new(),
            guide_path_id: None,
            variable_text: None,
            max_width: None,
            squeeze: false,
            ignore_empty_vars: false,
        }
    }

    #[test]
    fn bend_mode_produces_curved_geometry() {
        // Use tight radius (20mm) so the arc curvature is clearly visible
        let mut data = make_bend_text(20.0);
        refresh_text_object_cache(
            &mut data,
            &Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 40.0)),
        );
        if let ObjectData::Text {
            resolved_path_data, ..
        } = &data
        {
            assert!(
                resolved_path_data.is_some(),
                "bend_radius=20 should produce geometry"
            );
            // Verify we got actual curved geometry, not just straight text
            let path = VecPath::parse_svg_d(resolved_path_data.as_ref().unwrap());
            let bounds = path.bounds().unwrap();
            assert!(
                bounds.width() > 5.0,
                "curved text should have width: {:.1}",
                bounds.width()
            );
            assert!(
                bounds.height() > 5.0,
                "curved text should have height: {:.1}",
                bounds.height()
            );
        } else {
            panic!("expected text");
        }
    }

    #[test]
    fn bend_mode_negative_radius_flips() {
        let mut pos = make_bend_text(50.0);
        let mut neg = make_bend_text(-50.0);
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 40.0));
        refresh_text_object_cache(&mut pos, &bounds);
        refresh_text_object_cache(&mut neg, &bounds);
        let pos_d = match &pos {
            ObjectData::Text {
                resolved_path_data: Some(d),
                ..
            } => d.clone(),
            _ => panic!("expected resolved text"),
        };
        let neg_d = match &neg {
            ObjectData::Text {
                resolved_path_data: Some(d),
                ..
            } => d.clone(),
            _ => panic!("expected resolved text"),
        };
        assert_ne!(
            pos_d, neg_d,
            "positive vs negative radius should produce different geometry"
        );
    }

    #[test]
    fn bend_mode_with_distort() {
        let mut nodist = ObjectData::Text {
            content: "Hello".to_string(),
            font_family: "Arial".to_string(),
            font_size_mm: 10.0,
            alignment: TextAlignment::Left,
            alignment_v: TextAlignmentV::Top,
            bold: false,
            italic: false,
            upper_case: false,
            welded: false,
            h_spacing: 0.0,
            v_spacing: 0.0,
            on_path: false,
            path_offset: 0.0,
            distort: false,
            layout_mode: TextLayoutMode::Bend,
            rtl: false,
            bend_radius: 30.0,
            transform_style: crate::object::TextTransformStyle::None,
            transform_curve: 0.0,
            circle_placement: crate::object::TextCirclePlacement::TopOutside,
            resolved_font_source: None,
            resolved_font_key: None,
            resolved_path_data: None,
            missing_font: false,
            missing_glyphs: Vec::new(),
            guide_path_id: None,
            variable_text: None,
            max_width: None,
            squeeze: false,
            ignore_empty_vars: false,
        };
        let mut dist = nodist.clone();
        if let ObjectData::Text { distort, .. } = &mut dist {
            *distort = true;
        }
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 40.0));
        refresh_text_object_cache(&mut nodist, &bounds);
        refresh_text_object_cache(&mut dist, &bounds);
        let nodist_d = match &nodist {
            ObjectData::Text {
                resolved_path_data: Some(d),
                ..
            } => d.clone(),
            _ => panic!("expected resolved text"),
        };
        let dist_d = match &dist {
            ObjectData::Text {
                resolved_path_data: Some(d),
                ..
            } => d.clone(),
            _ => panic!("expected resolved text"),
        };
        assert_ne!(
            nodist_d, dist_d,
            "bend + distort should differ from bend alone on tight arc"
        );
    }

    #[test]
    fn bend_mode_preserves_properties() {
        // Bend with upper_case/h_spacing/welded should differ from default bend.
        // Note: alignment, alignment_v, and v_spacing are box-layout controls that
        // intentionally don't apply to path/bend modes (single-line curved text).
        let mut defaults = make_bend_text(50.0);
        let mut styled = ObjectData::Text {
            content: "Hello".to_string(),
            font_family: "Arial".to_string(),
            font_size_mm: 10.0,
            alignment: TextAlignment::Left,
            alignment_v: TextAlignmentV::Top,
            bold: false,
            italic: false,
            upper_case: true,
            welded: true,
            h_spacing: 3.0,
            v_spacing: 0.0,
            on_path: false,
            path_offset: 0.0,
            distort: false,
            layout_mode: TextLayoutMode::Bend,
            rtl: false,
            bend_radius: 50.0,
            transform_style: crate::object::TextTransformStyle::None,
            transform_curve: 0.0,
            circle_placement: crate::object::TextCirclePlacement::TopOutside,
            resolved_font_source: None,
            resolved_font_key: None,
            resolved_path_data: None,
            missing_font: false,
            missing_glyphs: Vec::new(),
            guide_path_id: None,
            variable_text: None,
            max_width: None,
            squeeze: false,
            ignore_empty_vars: false,
        };
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 40.0));
        refresh_text_object_cache(&mut defaults, &bounds);
        refresh_text_object_cache(&mut styled, &bounds);
        let defaults_d = match &defaults {
            ObjectData::Text {
                resolved_path_data: Some(d),
                ..
            } => d.clone(),
            _ => panic!("expected resolved text"),
        };
        let styled_d = match &styled {
            ObjectData::Text {
                resolved_path_data: Some(d),
                ..
            } => d.clone(),
            _ => panic!("expected resolved text"),
        };
        assert_ne!(
            defaults_d, styled_d,
            "bend with properties should differ from defaults"
        );
    }

    #[test]
    fn negative_h_spacing_narrows_text() {
        let normal = resolve_text_in_box(
            "Hello",
            "Arial",
            10.0,
            false,
            false,
            TextAlignment::Left,
            TextAlignmentV::Top,
            false,
            0.0,
            0.0,
            200.0,
            40.0,
        )
        .expect("normal text resolves");
        let tight = resolve_text_in_box(
            "Hello",
            "Arial",
            10.0,
            false,
            false,
            TextAlignment::Left,
            TextAlignmentV::Top,
            false,
            -1.0,
            0.0,
            200.0,
            40.0,
        )
        .expect("tight text resolves");
        let normal_w = normal.path.bounds().unwrap().width();
        let tight_w = tight.path.bounds().unwrap().width();
        assert!(
            tight_w < normal_w,
            "negative h_spacing should produce narrower text: {tight_w} vs {normal_w}"
        );
    }

    #[test]
    fn negative_v_spacing_compresses_lines() {
        let normal = resolve_text_in_box(
            "Line1\nLine2",
            "Arial",
            10.0,
            false,
            false,
            TextAlignment::Left,
            TextAlignmentV::Top,
            false,
            0.0,
            0.0,
            200.0,
            100.0,
        )
        .expect("normal multiline resolves");
        let tight = resolve_text_in_box(
            "Line1\nLine2",
            "Arial",
            10.0,
            false,
            false,
            TextAlignment::Left,
            TextAlignmentV::Top,
            false,
            0.0,
            -3.0,
            200.0,
            100.0,
        )
        .expect("tight multiline resolves");
        let normal_h = normal.path.bounds().unwrap().height();
        let tight_h = tight.path.bounds().unwrap().height();
        assert!(
            tight_h < normal_h,
            "negative v_spacing should produce shorter text block: {tight_h} vs {normal_h}"
        );
    }

    #[test]
    fn max_width_wraps_straight_text() {
        let normal = resolve_text_in_box(
            "A A A A",
            "Arial",
            10.0,
            false,
            false,
            TextAlignment::Left,
            TextAlignmentV::Top,
            false,
            0.0,
            0.0,
            200.0,
            80.0,
        )
        .expect("normal text resolves");
        let constrained = resolve_text_in_box_with_options(
            "A A A A",
            "Arial",
            10.0,
            false,
            false,
            TextAlignment::Left,
            TextAlignmentV::Top,
            false,
            0.0,
            0.0,
            200.0,
            80.0,
            Some(12.0),
            false,
            true,
        )
        .expect("constrained text resolves");
        let normal_bounds = normal.path.bounds().unwrap();
        let constrained_bounds = constrained.path.bounds().unwrap();
        assert!(
            constrained_bounds.width() <= 12.5,
            "constrained width should fit max_width"
        );
        assert!(
            constrained_bounds.height() > normal_bounds.height(),
            "wrapping should increase text block height"
        );
    }

    #[test]
    fn squeeze_compresses_width_without_wrapping() {
        let normal = resolve_text_in_box_with_options(
            "Hello",
            "Arial",
            10.0,
            false,
            false,
            TextAlignment::Left,
            TextAlignmentV::Top,
            false,
            0.0,
            0.0,
            200.0,
            40.0,
            Some(10.0),
            false,
            false,
        )
        .expect("normal text resolves");
        let squeezed = resolve_text_in_box_with_options(
            "Hello",
            "Arial",
            10.0,
            false,
            false,
            TextAlignment::Left,
            TextAlignmentV::Top,
            false,
            0.0,
            0.0,
            200.0,
            40.0,
            Some(10.0),
            true,
            false,
        )
        .expect("squeezed text resolves");
        let normal_bounds = normal.path.bounds().unwrap();
        let squeezed_bounds = squeezed.path.bounds().unwrap();
        assert!(
            squeezed_bounds.width() < normal_bounds.width(),
            "squeeze should reduce width"
        );
        assert!(
            squeezed_bounds.height() <= normal_bounds.height() + 0.01,
            "squeeze should preserve height"
        );
    }

    #[test]
    fn ignore_empty_vars_strips_unresolved_merge_fields() {
        let content = resolve_text_content_for_object(
            "unused",
            Some(&VariableTextConfig {
                template: "SN-{CSV:Name}-{Const:Missing}".to_string(),
                mode: None,
                offset: None,
                source: VariableTextSource {
                    csv_path: None,
                    csv_data: vec![vec!["Name".to_string()], vec![String::new()]],
                    field_defaults: HashMap::new(),
                    current: 0,
                    start: 0,
                    end: 0,
                    advance_by: 1,
                    auto_advance: false,
                    total_copies: 1,
                },
            }),
            true,
        );
        assert_eq!(
            content, "SN--",
            "empty variable fields should be removed from resolved text"
        );
    }

    #[test]
    fn intrinsic_text_bounds_grow_with_font_size() {
        let small = ObjectData::Text {
            content: "Hello".to_string(),
            font_family: "Arial".to_string(),
            font_size_mm: 5.0,
            alignment: TextAlignment::Left,
            alignment_v: TextAlignmentV::Top,
            bold: false,
            italic: false,
            upper_case: false,
            welded: false,
            h_spacing: 0.0,
            v_spacing: 0.0,
            on_path: false,
            path_offset: 0.0,
            distort: false,
            layout_mode: TextLayoutMode::Straight,
            rtl: false,
            bend_radius: 0.0,
            transform_style: crate::object::TextTransformStyle::None,
            transform_curve: 0.0,
            circle_placement: crate::object::TextCirclePlacement::TopOutside,
            resolved_font_source: None,
            resolved_font_key: None,
            resolved_path_data: None,
            missing_font: false,
            missing_glyphs: Vec::new(),
            guide_path_id: None,
            variable_text: None,
            max_width: None,
            squeeze: false,
            ignore_empty_vars: false,
        };
        let mut large = small.clone();
        if let ObjectData::Text { font_size_mm, .. } = &mut large {
            *font_size_mm = 20.0;
        }
        let small_bounds = intrinsic_text_bounds(&small).unwrap();
        let large_bounds = intrinsic_text_bounds(&large).unwrap();
        assert!(large_bounds.width() > small_bounds.width());
        assert!(large_bounds.height() > small_bounds.height());
    }

    #[test]
    fn font_resolution_exact_match_not_missing() {
        // Find a system font that actually resolves via fontdb query, then verify
        // our missing_font detection reports it as NOT missing.
        let fonts = available_font_families();
        let resolved = fonts.iter().find_map(|name| {
            resolve_with_system_font(
                "A",
                name,
                10.0,
                false,
                false,
                TextAlignment::Left,
                TextAlignmentV::Top,
                false,
                0.0,
                0.0,
                100.0,
                100.0,
                None,
                false,
                true,
            )
            .map(|r| (name.clone(), r))
        });
        let (font_name, r) = resolved.expect("at least one system font should resolve");
        assert!(
            !r.missing_font,
            "exact match for '{font_name}' should not be missing"
        );
        assert_eq!(r.resolved_font_source, TextFontSource::System);
    }

    #[test]
    fn font_resolution_generic_not_missing() {
        let resolved = resolve_text_in_box_with_options(
            "A",
            "sans-serif",
            10.0,
            false,
            false,
            TextAlignment::Left,
            TextAlignmentV::Top,
            false,
            0.0,
            0.0,
            100.0,
            100.0,
            None,
            false,
            true,
        );
        assert!(resolved.is_some(), "sans-serif should resolve");
        assert!(
            !resolved.unwrap().missing_font,
            "generic family should not be missing even when it uses the bundled fallback"
        );
    }

    #[test]
    fn selected_font_supporting_all_glyphs_has_no_missing_glyphs() {
        let Some(font_name) = system_font_supporting('中') else {
            return;
        };
        let resolved = resolve_text_in_box_with_options(
            "中",
            &font_name,
            10.0,
            false,
            false,
            TextAlignment::Left,
            TextAlignmentV::Top,
            false,
            0.0,
            0.0,
            100.0,
            100.0,
            None,
            false,
            true,
        )
        .expect("font with CJK coverage should resolve");

        assert!(
            resolved.missing_glyphs.is_empty(),
            "covering font should not report missing glyphs"
        );
        assert!(!resolved.path.is_empty());
    }

    #[test]
    fn system_fallback_supplies_chinese_for_latin_font_without_overlap_collapse() {
        let Some(latin_font) = system_font_lacking('中') else {
            return;
        };
        if system_font_supporting('中').is_none() {
            return;
        }

        let mixed = resolve_text_in_box_with_options(
            "A中B",
            &latin_font,
            10.0,
            false,
            false,
            TextAlignment::Left,
            TextAlignmentV::Top,
            false,
            0.0,
            0.0,
            200.0,
            100.0,
            None,
            false,
            true,
        )
        .expect("mixed Latin/CJK text should resolve with fallback");
        let latin_only = resolve_text_in_box_with_options(
            "AB",
            &latin_font,
            10.0,
            false,
            false,
            TextAlignment::Left,
            TextAlignmentV::Top,
            false,
            0.0,
            0.0,
            200.0,
            100.0,
            None,
            false,
            true,
        )
        .expect("Latin-only text should resolve");

        assert!(
            mixed.missing_glyphs.is_empty(),
            "system fallback should cover the Chinese glyph"
        );
        assert!(
            mixed.glyph_starts.len() >= 3,
            "mixed text should preserve separate positioned glyph groups"
        );
        let mixed_width = mixed.path.bounds().unwrap().width();
        let latin_width = latin_only.path.bounds().unwrap().width();
        assert!(
            mixed_width > latin_width,
            "fallback glyph should contribute to measured width: {mixed_width} <= {latin_width}"
        );
    }

    #[test]
    fn missing_glyphs_are_recomputed_and_clear_on_cache_refresh() {
        let mut data = text_data("A", "sans-serif");
        if let ObjectData::Text { missing_glyphs, .. } = &mut data {
            missing_glyphs.push("中".to_string());
        }
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 40.0));

        refresh_text_object_cache(&mut data, &bounds);
        let ObjectData::Text { missing_glyphs, .. } = &data else {
            panic!("expected text data");
        };
        assert!(
            missing_glyphs.is_empty(),
            "missing glyph warning should clear after a covering text refresh"
        );
    }

    #[test]
    fn font_resolution_unknown_font_reports_missing() {
        let resolved = resolve_with_system_font(
            "A",
            "NonExistentFont12345",
            10.0,
            false,
            false,
            TextAlignment::Left,
            TextAlignmentV::Top,
            false,
            0.0,
            0.0,
            100.0,
            100.0,
            None,
            false,
            true,
        );
        // fontdb may match a SansSerif fallback, or may return None
        if let Some(r) = resolved {
            assert!(
                r.missing_font,
                "fallback-matched font should report missing_font: true"
            );
            assert_eq!(r.resolved_font_source, TextFontSource::System);
        }
        // If None, the full pipeline would fall through to BundledFallback (also missing)
    }
}

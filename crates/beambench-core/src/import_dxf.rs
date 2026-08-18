//! DXF file import for common 2D laser geometry.

use beambench_common::path::{PathCommand, SubPath, VecPath};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const DXF_CURVE_TOLERANCE_MM: f64 = 0.02;
const MAX_CURVE_SUBDIVISION_DEPTH: u8 = 18;

/// A single entity extracted from a DXF file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DxfEntity {
    pub layer_name: String,
    pub path: VecPath,
}

/// Geometry recovered from a DXF plus entity types that could not be imported.
/// Keeping this report separate from the convenience `parse_dxf` API lets the
/// service warn users about partial imports instead of silently losing data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DxfParseReport {
    pub entities: Vec<DxfEntity>,
    pub skipped_entities: BTreeMap<String, usize>,
}

/// Parse DXF text content and extract vector paths.
/// Supports LINE, POLYLINE, LWPOLYLINE, CIRCLE, ARC, and SPLINE entities.
pub fn parse_dxf(content: &str) -> Result<Vec<DxfEntity>, String> {
    Ok(parse_dxf_with_report(content)?.entities)
}

/// Parse DXF text content and retain a count of unsupported or malformed
/// entities. This parser is implemented in-tree and has no third-party DXF
/// runtime or license dependency.
pub fn parse_dxf_with_report(content: &str) -> Result<DxfParseReport, String> {
    let pairs = parse_dxf_pairs(content);
    if !content.trim().is_empty() && pairs.is_empty() {
        return Err("File is not a valid ASCII DXF".to_string());
    }
    let scale = dxf_insunits_scale_to_mm(&pairs);
    let curve_tolerance = DXF_CURVE_TOLERANCE_MM / scale.abs().max(f64::EPSILON);
    let mut entities = Vec::new();
    let mut skipped_entities = BTreeMap::new();
    let mut i = 0;

    while i < pairs.len() {
        let (code, value) = &pairs[i];

        // Look for entity section
        if *code == 0 && value == "SECTION" {
            i += 1;
            if i < pairs.len() && pairs[i].0 == 2 && pairs[i].1 == "ENTITIES" {
                i += 1;
                // Parse entities until ENDSEC
                while i < pairs.len() {
                    if pairs[i].0 == 0 && pairs[i].1 == "ENDSEC" {
                        break;
                    }
                    if pairs[i].0 == 0 {
                        let entity_type = pairs[i].1.clone();
                        if let Some((entity, consumed)) = parse_entity(&pairs[i..], curve_tolerance)
                        {
                            entities.push(entity);
                            i += consumed;
                        } else {
                            if !matches!(entity_type.as_str(), "SEQEND" | "ENDSEC") {
                                *skipped_entities.entry(entity_type).or_insert(0) += 1;
                            }
                            i += skipped_entity_pair_len(&pairs[i..]);
                        }
                    } else {
                        i += 1;
                    }
                }
            }
        }
        i += 1;
    }

    // Scale all coordinates to millimeters per the file's declared units.
    if scale != 1.0 {
        for entity in &mut entities {
            for subpath in &mut entity.path.subpaths {
                for command in &mut subpath.commands {
                    match command {
                        PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => {
                            *x *= scale;
                            *y *= scale;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(DxfParseReport {
        entities,
        skipped_entities,
    })
}

/// Millimeter scale factor for the file's `$INSUNITS` header variable.
/// DXF unit codes: 1=inches, 2=feet, 4=mm, 5=cm, 6=m. Anything else
/// (including 0 = unitless or a missing header) is treated as already mm.
fn dxf_insunits_scale_to_mm(pairs: &[(i32, String)]) -> f64 {
    for (i, pair) in pairs.iter().enumerate() {
        if pair.0 == 9
            && pair.1 == "$INSUNITS"
            && let Some(next) = pairs.get(i + 1)
            && next.0 == 70
        {
            return match next.1.parse::<i32>().unwrap_or(0) {
                1 => 25.4,
                2 => 304.8,
                5 => 10.0,
                6 => 1000.0,
                _ => 1.0,
            };
        }
    }
    1.0
}

/// Parse DXF group codes (integer code + value pairs separated by newlines).
fn parse_dxf_pairs(content: &str) -> Vec<(i32, String)> {
    let lines: Vec<&str> = content.lines().collect();
    let mut pairs = Vec::new();
    let mut i = 0;

    while i + 1 < lines.len() {
        if let Ok(code) = lines[i].trim().parse::<i32>() {
            let value = lines[i + 1].trim().to_string();
            pairs.push((code, value));
            i += 2;
        } else {
            i += 1;
        }
    }

    pairs
}

/// Parse a single DXF entity from pairs, returning (entity, consumed_count).
fn parse_entity(pairs: &[(i32, String)], curve_tolerance: f64) -> Option<(DxfEntity, usize)> {
    if pairs.is_empty() || pairs[0].0 != 0 {
        return None;
    }

    let entity_type = &pairs[0].1;
    match entity_type.as_str() {
        "LINE" => parse_line(pairs),
        "CIRCLE" => parse_circle(pairs, curve_tolerance),
        "ARC" => parse_arc(pairs, curve_tolerance),
        "LWPOLYLINE" => parse_lwpolyline(pairs, curve_tolerance),
        "POLYLINE" => parse_polyline(pairs, curve_tolerance),
        "SPLINE" => parse_spline(pairs, curve_tolerance),
        _ => None,
    }
}

fn entity_pair_len(pairs: &[(i32, String)]) -> usize {
    pairs
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(index, pair)| (pair.0 == 0).then_some(index))
        .unwrap_or(pairs.len())
        .max(1)
}

/// Return the full span of an unsupported entity. Legacy POLYLINE containers
/// own their following VERTEX records through SEQEND, so count and skip the
/// container once rather than reporting each child as a separate entity.
fn skipped_entity_pair_len(pairs: &[(i32, String)]) -> usize {
    let header_len = entity_pair_len(pairs);
    if pairs.first().is_none_or(|pair| pair.1 != "POLYLINE") {
        return header_len;
    }

    let mut consumed = header_len;
    while consumed < pairs.len() && pairs[consumed].0 == 0 {
        match pairs[consumed].1.as_str() {
            "VERTEX" => consumed += entity_pair_len(&pairs[consumed..]),
            "SEQEND" => {
                consumed += entity_pair_len(&pairs[consumed..]);
                break;
            }
            _ => break,
        }
    }
    consumed.max(1)
}

#[allow(clippy::needless_range_loop)]
fn parse_line(pairs: &[(i32, String)]) -> Option<(DxfEntity, usize)> {
    let mut x1 = 0.0;
    let mut y1 = 0.0;
    let mut x2 = 0.0;
    let mut y2 = 0.0;
    let mut layer = "0".to_string();
    let mut consumed = 1;

    for i in 1..pairs.len() {
        match pairs[i].0 {
            0 => break, // Next entity
            8 => layer = pairs[i].1.clone(),
            10 => x1 = pairs[i].1.parse().unwrap_or(0.0),
            20 => y1 = pairs[i].1.parse().unwrap_or(0.0),
            11 => x2 = pairs[i].1.parse().unwrap_or(0.0),
            21 => y2 = pairs[i].1.parse().unwrap_or(0.0),
            _ => {}
        }
        consumed = i + 1;
    }

    let mut sp = SubPath::new();
    sp.commands.push(PathCommand::MoveTo { x: x1, y: y1 });
    sp.commands.push(PathCommand::LineTo { x: x2, y: y2 });

    Some((
        DxfEntity {
            layer_name: layer,
            path: VecPath { subpaths: vec![sp] },
        },
        consumed,
    ))
}

#[allow(clippy::needless_range_loop)]
fn parse_circle(pairs: &[(i32, String)], curve_tolerance: f64) -> Option<(DxfEntity, usize)> {
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut radius: f64 = 0.0;
    let mut layer = "0".to_string();
    let mut consumed = 1;

    for i in 1..pairs.len() {
        match pairs[i].0 {
            0 => break,
            8 => layer = pairs[i].1.clone(),
            10 => cx = pairs[i].1.parse().unwrap_or(0.0),
            20 => cy = pairs[i].1.parse().unwrap_or(0.0),
            40 => radius = pairs[i].1.parse().unwrap_or(0.0),
            _ => {}
        }
        consumed = i + 1;
    }

    if !radius.is_finite() || radius <= 0.0 {
        return None;
    }

    let segments = arc_segment_count(radius, std::f64::consts::TAU, curve_tolerance);
    let mut sp = SubPath::new();

    for i in 0..segments {
        let angle = 2.0 * std::f64::consts::PI * (i as f64) / (segments as f64);
        let x = cx + radius * angle.cos();
        let y = cy + radius * angle.sin();

        if i == 0 {
            sp.commands.push(PathCommand::MoveTo { x, y });
        } else {
            sp.commands.push(PathCommand::LineTo { x, y });
        }
    }
    sp.commands.push(PathCommand::Close);
    sp.closed = true;

    Some((
        DxfEntity {
            layer_name: layer,
            path: VecPath { subpaths: vec![sp] },
        },
        consumed,
    ))
}

#[allow(clippy::needless_range_loop)]
fn parse_arc(pairs: &[(i32, String)], curve_tolerance: f64) -> Option<(DxfEntity, usize)> {
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut radius: f64 = 0.0;
    let mut start_angle: f64 = 0.0;
    let mut end_angle: f64 = 360.0;
    let mut layer = "0".to_string();
    let mut consumed = 1;

    for i in 1..pairs.len() {
        match pairs[i].0 {
            0 => break,
            8 => layer = pairs[i].1.clone(),
            10 => cx = pairs[i].1.parse().unwrap_or(0.0),
            20 => cy = pairs[i].1.parse().unwrap_or(0.0),
            40 => radius = pairs[i].1.parse().unwrap_or(0.0),
            50 => start_angle = pairs[i].1.parse().unwrap_or(0.0),
            51 => end_angle = pairs[i].1.parse().unwrap_or(360.0),
            _ => {}
        }
        consumed = i + 1;
    }

    if !radius.is_finite() || radius <= 0.0 {
        return None;
    }

    // DXF arcs run counter-clockwise. An end angle below the start angle wraps
    // through 360 degrees rather than reversing direction.
    let start_rad = start_angle.to_radians();
    let mut sweep = (end_angle - start_angle)
        .to_radians()
        .rem_euclid(std::f64::consts::TAU);
    if sweep.abs() <= f64::EPSILON {
        sweep = std::f64::consts::TAU;
    }
    let segments = arc_segment_count(radius, sweep, curve_tolerance);
    let delta = sweep / segments as f64;

    let mut sp = SubPath::new();

    for i in 0..=segments {
        let angle = start_rad + delta * (i as f64);
        let x = cx + radius * angle.cos();
        let y = cy + radius * angle.sin();

        if i == 0 {
            sp.commands.push(PathCommand::MoveTo { x, y });
        } else {
            sp.commands.push(PathCommand::LineTo { x, y });
        }
    }

    Some((
        DxfEntity {
            layer_name: layer,
            path: VecPath { subpaths: vec![sp] },
        },
        consumed,
    ))
}

#[allow(clippy::needless_range_loop)]
fn parse_lwpolyline(pairs: &[(i32, String)], curve_tolerance: f64) -> Option<(DxfEntity, usize)> {
    let mut vertices: Vec<PolylineVertex> = Vec::new();
    let mut layer = "0".to_string();
    let mut consumed = 1;
    let mut current: Option<PolylineVertex> = None;
    let mut closed = false;

    for i in 1..pairs.len() {
        match pairs[i].0 {
            0 => break,
            8 => layer = pairs[i].1.clone(),
            // Polyline flag: bit 0 = closed
            70 => closed = (pairs[i].1.trim().parse::<i32>().unwrap_or(0) & 1) != 0,
            10 => {
                if let Some(vertex) = current.take() {
                    vertices.push(vertex);
                }
                current = Some(PolylineVertex {
                    x: pairs[i].1.parse().unwrap_or(0.0),
                    y: 0.0,
                    bulge: 0.0,
                });
            }
            20 => {
                if let Some(vertex) = current.as_mut() {
                    vertex.y = pairs[i].1.parse().unwrap_or(0.0);
                }
            }
            42 => {
                if let Some(vertex) = current.as_mut() {
                    vertex.bulge = pairs[i].1.parse().unwrap_or(0.0);
                }
            }
            _ => {}
        }
        consumed = i + 1;
    }
    if let Some(vertex) = current {
        vertices.push(vertex);
    }

    let sp = polyline_subpath(&vertices, closed, curve_tolerance)?;

    Some((
        DxfEntity {
            layer_name: layer,
            path: VecPath { subpaths: vec![sp] },
        },
        consumed,
    ))
}

#[derive(Debug, Clone, Copy)]
struct PolylineVertex {
    x: f64,
    y: f64,
    /// Tangent of one quarter of the signed included arc angle.
    bulge: f64,
}

fn parse_polyline(pairs: &[(i32, String)], curve_tolerance: f64) -> Option<(DxfEntity, usize)> {
    let header_len = entity_pair_len(pairs);
    let mut layer = "0".to_string();
    let mut flags = 0_i32;
    for pair in &pairs[1..header_len] {
        match pair.0 {
            8 => layer = pair.1.clone(),
            70 => flags = pair.1.parse().unwrap_or(0),
            _ => {}
        }
    }

    // Polygon meshes and polyface meshes need face topology rather than the
    // simple ordered-vertex interpretation used for laser paths.
    if flags & (16 | 64) != 0 {
        return None;
    }

    let mut vertices = Vec::new();
    let mut consumed = header_len;
    while consumed < pairs.len() {
        if pairs[consumed].0 != 0 {
            consumed += 1;
            continue;
        }
        match pairs[consumed].1.as_str() {
            "VERTEX" => {
                let vertex_len = entity_pair_len(&pairs[consumed..]);
                let mut vertex = PolylineVertex {
                    x: 0.0,
                    y: 0.0,
                    bulge: 0.0,
                };
                for pair in &pairs[consumed + 1..consumed + vertex_len] {
                    match pair.0 {
                        10 => vertex.x = pair.1.parse().unwrap_or(0.0),
                        20 => vertex.y = pair.1.parse().unwrap_or(0.0),
                        42 => vertex.bulge = pair.1.parse().unwrap_or(0.0),
                        _ => {}
                    }
                }
                vertices.push(vertex);
                consumed += vertex_len;
            }
            "SEQEND" => {
                consumed += entity_pair_len(&pairs[consumed..]);
                break;
            }
            _ => break,
        }
    }

    let closed = flags & 1 != 0;
    let sp = polyline_subpath(&vertices, closed, curve_tolerance)?;
    Some((
        DxfEntity {
            layer_name: layer,
            path: VecPath { subpaths: vec![sp] },
        },
        consumed.max(1),
    ))
}

fn polyline_subpath(
    vertices: &[PolylineVertex],
    closed: bool,
    curve_tolerance: f64,
) -> Option<SubPath> {
    let first = *vertices.first()?;
    if vertices.len() < 2
        || vertices
            .iter()
            .any(|vertex| !vertex.x.is_finite() || !vertex.y.is_finite())
    {
        return None;
    }

    let mut sp = SubPath::new();
    sp.commands.push(PathCommand::MoveTo {
        x: first.x,
        y: first.y,
    });

    let segment_count = if closed {
        vertices.len()
    } else {
        vertices.len() - 1
    };
    for index in 0..segment_count {
        let start = vertices[index];
        let end = vertices[(index + 1) % vertices.len()];
        if closed && index + 1 == segment_count && start.bulge.abs() <= 1e-12 {
            // Close draws the final straight segment without duplicating the
            // first point. Curved closing segments still need tessellation.
            continue;
        }
        append_bulge_segment(&mut sp, start, end, curve_tolerance);
    }

    if closed {
        // The final bulge segment already reaches the first vertex. Close keeps
        // the contour's topology explicit without adding another line.
        sp.commands.push(PathCommand::Close);
        sp.closed = true;
    }
    Some(sp)
}

fn append_bulge_segment(
    sp: &mut SubPath,
    start: PolylineVertex,
    end: PolylineVertex,
    curve_tolerance: f64,
) {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let chord = dx.hypot(dy);
    let bulge = start.bulge;
    if chord <= f64::EPSILON || !bulge.is_finite() || bulge.abs() <= 1e-12 {
        sp.commands.push(PathCommand::LineTo { x: end.x, y: end.y });
        return;
    }

    let sweep = 4.0 * bulge.atan();
    let radius = chord * (1.0 + bulge * bulge) / (4.0 * bulge.abs());
    let center_offset = chord * (1.0 - bulge * bulge) / (4.0 * bulge);
    let mid_x = (start.x + end.x) * 0.5;
    let mid_y = (start.y + end.y) * 0.5;
    let center_x = mid_x - dy / chord * center_offset;
    let center_y = mid_y + dx / chord * center_offset;
    let start_angle = (start.y - center_y).atan2(start.x - center_x);
    let segments = arc_segment_count(radius, sweep.abs(), curve_tolerance);

    for step in 1..=segments {
        if step == segments {
            sp.commands.push(PathCommand::LineTo { x: end.x, y: end.y });
        } else {
            let angle = start_angle + sweep * step as f64 / segments as f64;
            sp.commands.push(PathCommand::LineTo {
                x: center_x + radius * angle.cos(),
                y: center_y + radius * angle.sin(),
            });
        }
    }
}

fn arc_segment_count(radius: f64, sweep: f64, tolerance: f64) -> usize {
    if !radius.is_finite() || radius <= f64::EPSILON || sweep <= f64::EPSILON {
        return 1;
    }
    let tolerance = tolerance.max(1e-9).min(radius);
    let max_angle = (2.0 * (1.0 - tolerance / radius).clamp(-1.0, 1.0).acos())
        .max(std::f64::consts::PI / 2048.0);
    (sweep / max_angle).ceil().clamp(1.0, 4096.0) as usize
}

fn parse_spline(pairs: &[(i32, String)], curve_tolerance: f64) -> Option<(DxfEntity, usize)> {
    let mut layer = "0".to_string();
    let mut flags = 0_i32;
    let mut degree = None;
    let mut declared_knots = None;
    let mut declared_control_points = None;
    let mut knots = Vec::new();
    let mut parsed_weights = Vec::new();
    let mut control_points = Vec::new();
    let mut current_control: Option<(f64, f64)> = None;
    let consumed = entity_pair_len(pairs);

    for pair in &pairs[1..consumed] {
        match pair.0 {
            8 => layer = pair.1.clone(),
            70 => flags = pair.1.parse().unwrap_or(0),
            71 => degree = pair.1.parse::<usize>().ok(),
            72 => declared_knots = pair.1.parse::<usize>().ok(),
            73 => declared_control_points = pair.1.parse::<usize>().ok(),
            40 => knots.push(pair.1.parse::<f64>().ok()?),
            41 => parsed_weights.push(pair.1.parse::<f64>().ok()?),
            10 => {
                if let Some(point) = current_control.take() {
                    control_points.push(point);
                }
                current_control = Some((pair.1.parse::<f64>().ok()?, 0.0));
            }
            20 => {
                if let Some(point) = current_control.as_mut() {
                    point.1 = pair.1.parse::<f64>().ok()?;
                }
            }
            _ => {}
        }
    }
    if let Some(point) = current_control {
        control_points.push(point);
    }

    let degree = degree?;
    if degree == 0
        || control_points.len() <= degree
        || knots.len() < control_points.len() + degree + 1
        || declared_knots.is_some_and(|count| count != knots.len())
        || declared_control_points.is_some_and(|count| count != control_points.len())
        || control_points
            .iter()
            .any(|point| !point.0.is_finite() || !point.1.is_finite())
        || knots.iter().any(|knot| !knot.is_finite())
        || knots.windows(2).any(|window| window[0] > window[1])
    {
        return None;
    }

    let weights = if parsed_weights.is_empty() {
        vec![1.0; control_points.len()]
    } else if parsed_weights.len() == control_points.len()
        && parsed_weights
            .iter()
            .all(|weight| weight.is_finite() && *weight > 0.0)
    {
        parsed_weights
    } else {
        return None;
    };

    let closed = flags & 1 != 0;
    let sp = spline_subpath(
        &control_points,
        &weights,
        &knots,
        degree,
        closed,
        curve_tolerance,
    )?;
    Some((
        DxfEntity {
            layer_name: layer,
            path: VecPath { subpaths: vec![sp] },
        },
        consumed,
    ))
}

fn spline_subpath(
    control_points: &[(f64, f64)],
    weights: &[f64],
    knots: &[f64],
    degree: usize,
    closed: bool,
    curve_tolerance: f64,
) -> Option<SubPath> {
    let last_control = control_points.len().checked_sub(1)?;
    let domain_start = *knots.get(degree)?;
    let domain_end = *knots.get(last_control + 1)?;
    if domain_end <= domain_start {
        return None;
    }

    let evaluate =
        |parameter| evaluate_rational_bspline(control_points, weights, knots, degree, parameter);
    let start = evaluate(domain_start)?;
    let mut sampled = vec![start];

    // Subdivide each non-empty knot span independently. Knot boundaries are
    // natural places for curvature changes and make adaptive flattening both
    // accurate and deterministic.
    for span in degree..=last_control {
        let span_start = knots[span].max(domain_start);
        let span_end = knots[span + 1].min(domain_end);
        if span_end <= span_start {
            continue;
        }
        let p0 = evaluate(span_start)?;
        let p1 = evaluate(span_end)?;
        append_adaptive_spline_points(
            &evaluate,
            span_start,
            p0,
            span_end,
            p1,
            curve_tolerance,
            0,
            &mut sampled,
        )?;
    }

    if sampled.len() < 2 {
        return None;
    }
    let mut sp = SubPath::new();
    sp.commands.push(PathCommand::MoveTo {
        x: sampled[0].0,
        y: sampled[0].1,
    });
    for &(x, y) in &sampled[1..] {
        sp.commands.push(PathCommand::LineTo { x, y });
    }
    if closed {
        sp.commands.push(PathCommand::Close);
        sp.closed = true;
    }
    Some(sp)
}

#[allow(clippy::too_many_arguments)]
fn append_adaptive_spline_points<F>(
    evaluate: &F,
    t0: f64,
    p0: (f64, f64),
    t1: f64,
    p1: (f64, f64),
    tolerance: f64,
    depth: u8,
    output: &mut Vec<(f64, f64)>,
) -> Option<()>
where
    F: Fn(f64) -> Option<(f64, f64)>,
{
    let quarter_t = t0 + (t1 - t0) * 0.25;
    let mid_t = (t0 + t1) * 0.5;
    let three_quarter_t = t0 + (t1 - t0) * 0.75;
    let quarter = evaluate(quarter_t)?;
    let mid = evaluate(mid_t)?;
    let three_quarter = evaluate(three_quarter_t)?;
    let flatness = point_line_distance(quarter, p0, p1)
        .max(point_line_distance(mid, p0, p1))
        .max(point_line_distance(three_quarter, p0, p1));

    if flatness <= tolerance || depth >= MAX_CURVE_SUBDIVISION_DEPTH {
        if output
            .last()
            .is_none_or(|last| point_distance(*last, p1) > 1e-12)
        {
            output.push(p1);
        }
        return Some(());
    }

    append_adaptive_spline_points(evaluate, t0, p0, mid_t, mid, tolerance, depth + 1, output)?;
    append_adaptive_spline_points(evaluate, mid_t, mid, t1, p1, tolerance, depth + 1, output)
}

fn evaluate_rational_bspline(
    control_points: &[(f64, f64)],
    weights: &[f64],
    knots: &[f64],
    degree: usize,
    parameter: f64,
) -> Option<(f64, f64)> {
    let last_control = control_points.len().checked_sub(1)?;
    let domain_end = *knots.get(last_control + 1)?;
    let span = if parameter >= domain_end {
        last_control
    } else {
        (degree..=last_control)
            .find(|&index| knots[index] <= parameter && parameter < knots[index + 1])?
    };

    // De Boor in homogeneous coordinates handles ordinary and rational
    // B-splines with the same stable recurrence.
    let mut points = Vec::with_capacity(degree + 1);
    for local in 0..=degree {
        let index = span.checked_sub(degree)? + local;
        let weight = *weights.get(index)?;
        let point = *control_points.get(index)?;
        points.push((point.0 * weight, point.1 * weight, weight));
    }
    for level in 1..=degree {
        for local in (level..=degree).rev() {
            let index = span - degree + local;
            let denominator = knots[index + degree - level + 1] - knots[index];
            let alpha = if denominator.abs() <= f64::EPSILON {
                0.0
            } else {
                ((parameter - knots[index]) / denominator).clamp(0.0, 1.0)
            };
            points[local] = (
                (1.0 - alpha) * points[local - 1].0 + alpha * points[local].0,
                (1.0 - alpha) * points[local - 1].1 + alpha * points[local].1,
                (1.0 - alpha) * points[local - 1].2 + alpha * points[local].2,
            );
        }
    }
    let result = points[degree];
    if result.2.abs() <= f64::EPSILON {
        return None;
    }
    let point = (result.0 / result.2, result.1 / result.2);
    (point.0.is_finite() && point.1.is_finite()).then_some(point)
}

fn point_distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn point_line_distance(point: (f64, f64), start: (f64, f64), end: (f64, f64)) -> f64 {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let denominator = dx * dx + dy * dy;
    if denominator <= f64::EPSILON {
        return point_distance(point, start);
    }
    let projection = ((point.0 - start.0) * dx + (point.1 - start.1) * dy) / denominator;
    let nearest = (start.0 + projection * dx, start.1 + projection * dy);
    point_distance(point, nearest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_line() {
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nLayer1\n10\n10.0\n20\n20.0\n11\n30.0\n21\n40.0\n0\nENDSEC\n";
        let entities = parse_dxf(dxf).unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].layer_name, "Layer1");
        assert_eq!(entities[0].path.subpaths.len(), 1);
        assert_eq!(entities[0].path.subpaths[0].commands.len(), 2);
    }

    #[test]
    fn parse_circle_creates_closed_path() {
        let dxf =
            "0\nSECTION\n2\nENTITIES\n0\nCIRCLE\n8\n0\n10\n50.0\n20\n50.0\n40\n25.0\n0\nENDSEC\n";
        let entities = parse_dxf(dxf).unwrap();
        assert_eq!(entities.len(), 1);
        assert!(entities[0].path.subpaths[0].closed);
        // Adaptive flattening must be finer than the old fixed 32-gon.
        assert!(entities[0].path.subpaths[0].commands.len() > 33);
    }

    #[test]
    fn parse_arc_creates_open_path() {
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nARC\n8\n0\n10\n0.0\n20\n0.0\n40\n10.0\n50\n0.0\n51\n90.0\n0\nENDSEC\n";
        let entities = parse_dxf(dxf).unwrap();
        assert_eq!(entities.len(), 1);
        assert!(!entities[0].path.subpaths[0].closed);
        assert!(entities[0].path.subpaths[0].commands.len() > 1);
    }

    #[test]
    fn parse_lwpolyline() {
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n8\n0\n10\n0.0\n20\n0.0\n10\n10.0\n20\n10.0\n10\n20.0\n20\n0.0\n0\nENDSEC\n";
        let entities = parse_dxf(dxf).unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].path.subpaths[0].commands.len(), 3);
        assert!(
            !entities[0].path.subpaths[0].closed,
            "open polyline (no flag) must stay open"
        );
    }

    #[test]
    fn parse_lwpolyline_closed_flag() {
        // Group code 70 = 1 (bit 0 set) marks the polyline as closed.
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n8\n0\n90\n3\n70\n1\n10\n0.0\n20\n0.0\n10\n10.0\n20\n10.0\n10\n20.0\n20\n0.0\n0\nENDSEC\n";
        let entities = parse_dxf(dxf).unwrap();
        assert_eq!(entities.len(), 1);
        let sp = &entities[0].path.subpaths[0];
        assert!(sp.closed, "code 70 bit 0 must close the subpath");
        assert_eq!(sp.commands.len(), 4); // M L L Z
        assert_eq!(sp.commands[3], PathCommand::Close);
    }

    #[test]
    fn parse_lwpolyline_plinegen_flag_not_closed() {
        // Code 70 = 128 (plinegen) has bit 0 clear — must not close.
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n8\n0\n70\n128\n10\n0.0\n20\n0.0\n10\n10.0\n20\n10.0\n0\nENDSEC\n";
        let entities = parse_dxf(dxf).unwrap();
        assert!(!entities[0].path.subpaths[0].closed);
    }

    #[test]
    fn parse_multiple_entities() {
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLINE\n10\n0.0\n20\n0.0\n11\n10.0\n21\n10.0\n0\nCIRCLE\n10\n5.0\n20\n5.0\n40\n2.0\n0\nENDSEC\n";
        let entities = parse_dxf(dxf).unwrap();
        assert_eq!(entities.len(), 2);
    }

    #[test]
    fn parse_empty_dxf() {
        let dxf = "";
        let entities = parse_dxf(dxf).unwrap();
        assert_eq!(entities.len(), 0);
    }

    #[test]
    fn dxf_insunits_inches_scales_to_mm() {
        // Header declares $INSUNITS = 1 (inches); a 1-unit line must become 25.4 mm.
        let dxf = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n1\n0\nENDSEC\n\
0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0.0\n20\n0.0\n11\n1.0\n21\n0.0\n0\nENDSEC\n0\nEOF\n";
        let entities = parse_dxf(dxf).unwrap();
        match entities[0].path.subpaths[0].commands[1] {
            PathCommand::LineTo { x, .. } => {
                assert!((x - 25.4).abs() < 1e-6, "expected 25.4mm, got {x}")
            }
            _ => panic!("expected LineTo"),
        }
    }

    #[test]
    fn dxf_insunits_absent_is_unscaled() {
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0.0\n20\n0.0\n11\n10.0\n21\n0.0\n0\nENDSEC\n0\nEOF\n";
        let entities = parse_dxf(dxf).unwrap();
        match entities[0].path.subpaths[0].commands[1] {
            PathCommand::LineTo { x, .. } => {
                assert!((x - 10.0).abs() < 1e-6, "expected 10mm, got {x}")
            }
            _ => panic!("expected LineTo"),
        }
    }

    #[test]
    fn dxf_insunits_cm_scales_to_mm() {
        // $INSUNITS = 5 (centimeters); 1 cm -> 10 mm.
        let dxf = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n5\n0\nENDSEC\n\
0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0.0\n20\n0.0\n11\n1.0\n21\n0.0\n0\nENDSEC\n0\nEOF\n";
        let entities = parse_dxf(dxf).unwrap();
        match entities[0].path.subpaths[0].commands[1] {
            PathCommand::LineTo { x, .. } => {
                assert!((x - 10.0).abs() < 1e-6, "expected 10mm, got {x}")
            }
            _ => panic!("expected LineTo"),
        }
    }

    #[test]
    fn parse_cubic_spline_preserves_endpoints_and_curvature() {
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nSPLINE\n8\nCurves\n70\n0\n71\n3\n72\n8\n73\n4\n74\n0\n40\n0\n40\n0\n40\n0\n40\n0\n40\n1\n40\n1\n40\n1\n40\n1\n10\n0\n20\n0\n30\n0\n10\n0\n20\n10\n30\n0\n10\n10\n20\n10\n30\n0\n10\n10\n20\n0\n30\n0\n0\nENDSEC\n0\nEOF\n";
        let entities = parse_dxf(dxf).unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].layer_name, "Curves");
        let commands = &entities[0].path.subpaths[0].commands;
        assert_eq!(commands[0], PathCommand::MoveTo { x: 0.0, y: 0.0 });
        assert!(commands.len() > 8, "curve should be adaptively subdivided");
        assert_eq!(
            commands.last(),
            Some(&PathCommand::LineTo { x: 10.0, y: 0.0 })
        );
        assert!(
            commands
                .iter()
                .any(|command| { matches!(command, PathCommand::LineTo { y, .. } if *y > 7.0) })
        );
    }

    #[test]
    fn parse_lwpolyline_preserves_bulge_arc() {
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n8\n0\n90\n2\n70\n0\n10\n0\n20\n0\n42\n1\n10\n10\n20\n0\n0\nENDSEC\n";
        let entities = parse_dxf(dxf).unwrap();
        let commands = &entities[0].path.subpaths[0].commands;
        assert!(commands.len() > 2, "bulge must not collapse to one line");
        assert!(
            commands.iter().any(|command| {
                matches!(command, PathCommand::LineTo { y, .. } if y.abs() > 1.0)
            })
        );
        assert_eq!(
            commands.last(),
            Some(&PathCommand::LineTo { x: 10.0, y: 0.0 })
        );
    }

    #[test]
    fn parse_legacy_polyline_vertices() {
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n8\nLegacy\n70\n1\n0\nVERTEX\n10\n0\n20\n0\n0\nVERTEX\n10\n10\n20\n0\n0\nVERTEX\n10\n10\n20\n10\n0\nSEQEND\n0\nENDSEC\n";
        let entities = parse_dxf(dxf).unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].layer_name, "Legacy");
        let subpath = &entities[0].path.subpaths[0];
        assert!(subpath.closed);
        assert_eq!(subpath.commands.last(), Some(&PathCommand::Close));
    }

    #[test]
    fn wrapped_arc_runs_counter_clockwise_across_zero_degrees() {
        let dxf =
            "0\nSECTION\n2\nENTITIES\n0\nARC\n10\n0\n20\n0\n40\n10\n50\n350\n51\n10\n0\nENDSEC\n";
        let entities = parse_dxf(dxf).unwrap();
        let commands = &entities[0].path.subpaths[0].commands;
        assert!(
            commands.len() < 20,
            "20-degree arc must not become a 340-degree arc"
        );
        assert!(
            commands
                .iter()
                .all(|command| { !matches!(command, PathCommand::LineTo { x, .. } if *x < 9.0) })
        );
    }

    #[test]
    fn report_counts_unsupported_entities_without_hiding_supported_geometry() {
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nTEXT\n10\n1\n20\n2\n1\nhello\n0\nLINE\n10\n0\n20\n0\n11\n5\n21\n0\n0\nENDSEC\n";
        let report = parse_dxf_with_report(dxf).unwrap();
        assert_eq!(report.entities.len(), 1);
        assert_eq!(report.skipped_entities.get("TEXT"), Some(&1));
    }

    #[test]
    fn skipped_mesh_polyline_is_counted_once_without_vertex_noise() {
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n8\nMesh\n70\n16\n0\nVERTEX\n10\n0\n20\n0\n0\nVERTEX\n10\n10\n20\n0\n0\nVERTEX\n10\n10\n20\n10\n0\nSEQEND\n0\nLINE\n10\n0\n20\n0\n11\n5\n21\n0\n0\nENDSEC\n";
        let report = parse_dxf_with_report(dxf).unwrap();

        assert_eq!(report.entities.len(), 1);
        assert_eq!(report.skipped_entities.get("POLYLINE"), Some(&1));
        assert!(!report.skipped_entities.contains_key("VERTEX"));
    }
}

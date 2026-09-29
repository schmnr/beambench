use super::*;
use geo::algorithm::bool_ops::FillRule;
use geo::{BooleanOps, Contains, Coord, LineString, MultiLineString, MultiPolygon, Polygon};
use lopdf::{
    Document, Object, Stream,
    content::{Content, Operation},
};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub struct PdfImage {
    pub png: Vec<u8>,
    /// Closed clipping geometry in PDF millimetres, applied at engraving resolution.
    pub clip: Option<VecPath>,
    /// Maps the unit square, with image row zero at the top, to PDF millimetres.
    pub transform: Transform2D,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PdfArtwork {
    /// Converts PDF millimetres to a top-left, Y-down page in millimetres.
    pub page_to_canvas: Transform2D,
    pub paths: Vec<PdfPaintedPath>,
    pub images: Vec<PdfImage>,
}

#[derive(Clone, Default)]
struct State {
    graphics: PdfGraphicsState,
    clip: Option<MultiPolygon<f64>>,
}

pub(super) struct Budget {
    bytes: usize,
    operations: usize,
    pixels: usize,
    points: usize,
}

fn charge(remaining: &mut usize, amount: usize, label: &str) -> Result<(), String> {
    *remaining = remaining
        .checked_sub(amount)
        .ok_or_else(|| format!("PDF {label} limit exceeded"))?;
    Ok(())
}

pub(super) fn extract(
    doc: &Document,
    resources: Option<&Object>,
    bytes: &[u8],
    remaining: usize,
    bounds: [f64; 4],
) -> Result<PdfArtwork, String> {
    let mut budget = Budget {
        bytes: remaining,
        operations: 100_000,
        pixels: 16_000_000,
        points: 200_000,
    };
    let mut output = PdfArtwork {
        page_to_canvas: Transform2D::translate(-bounds[0] * PT_TO_MM, bounds[3] * PT_TO_MM)
            .compose(&Transform2D::scale(1.0, -1.0)),
        paths: Vec::new(),
        images: Vec::new(),
    };
    let clip = geo::Rect::new(
        Coord {
            x: bounds[0],
            y: bounds[1],
        },
        Coord {
            x: bounds[2],
            y: bounds[3],
        },
    )
    .to_polygon();
    let initial = State {
        clip: Some(MultiPolygon(vec![clip])),
        ..Default::default()
    };
    walk(
        doc,
        resources,
        bytes,
        initial,
        &mut HashSet::new(),
        &mut budget,
        &mut output,
    )?;
    for path in &mut output.paths {
        scale_vecpath(&mut path.path, PT_TO_MM);
    }
    for image in &mut output.images {
        image.transform = Transform2D::scale(PT_TO_MM, PT_TO_MM).compose(&image.transform);
        if let Some(clip) = &mut image.clip {
            scale_vecpath(clip, PT_TO_MM);
        }
    }
    if output.paths.is_empty() && output.images.is_empty() {
        return Err("No artwork found in PDF".into());
    }
    Ok(output)
}

pub(super) fn numbers(values: &[Object], count: usize) -> Result<Vec<f64>, String> {
    if values.len() != count {
        return Err("Invalid PDF drawing operand count".into());
    }
    values
        .iter()
        .map(|value| {
            value
                .as_float()
                .map(f64::from)
                .ok()
                .filter(|v| v.is_finite() && v.abs() <= 10_000_000.0)
                .ok_or_else(|| "Invalid or excessive PDF drawing coordinate".into())
        })
        .collect()
}

fn matrix(values: &[Object]) -> Result<Transform2D, String> {
    let v = numbers(values, 6)?;
    Ok(Transform2D {
        a: v[0],
        b: v[1],
        c: v[2],
        d: v[3],
        tx: v[4],
        ty: v[5],
    })
}

fn cm(t: Transform2D) -> Operation {
    Operation::new(
        "cm",
        [t.a, t.b, t.c, t.d, t.tx, t.ty]
            .into_iter()
            .map(Object::from)
            .collect(),
    )
}

fn path_from_operations(
    ops: &[Operation],
    state: PdfGraphicsState,
    paint: Option<&Operation>,
) -> Result<Vec<PdfPaintedPath>, String> {
    let rgb = |c: PdfRgbColor| {
        vec![
            (f64::from(c.r) / 255.0).into(),
            (f64::from(c.g) / 255.0).into(),
            (f64::from(c.b) / 255.0).into(),
        ]
    };
    let mut operations = vec![
        Operation::new("RG", rgb(state.stroke_color)),
        Operation::new("rg", rgb(state.fill_color)),
    ];
    operations.extend_from_slice(ops);
    if let Some(paint) = paint {
        operations.push(paint.clone());
    }
    let encoded = Content { operations }.encode().map_err(|e| e.to_string())?;
    Ok(parse_content_stream(&String::from_utf8_lossy(&encoded)))
}

fn walk(
    doc: &Document,
    resources: Option<&Object>,
    bytes: &[u8],
    mut state: State,
    active: &mut HashSet<lopdf::ObjectId>,
    budget: &mut Budget,
    output: &mut PdfArtwork,
) -> Result<(), String> {
    if active.len() > 16 {
        return Err("PDF form nesting limit exceeded".into());
    }
    let content = Content::decode_with_operation_limit(bytes, budget.operations)
        .map_err(|e| format!("PDF drawing operation limit or malformed content: {e}"))?;
    charge(
        &mut budget.operations,
        content.operations.len(),
        "drawing operation",
    )?;
    let mut stack = Vec::new();
    let mut path = Vec::new();
    let mut pending_clip = None;
    for op in content.operations {
        let operator = op.operator.as_str();
        match operator {
            "q" => {
                if stack.len() >= 64 {
                    return Err("PDF graphics state nesting limit exceeded".into());
                }
                stack.push(state.clone());
            }
            "Q" => state = stack.pop().ok_or("Unbalanced PDF graphics state restore")?,
            "cm" => {
                state.graphics.ctm = state.graphics.ctm.compose(&matrix(&op.operands)?);
                let t = state.graphics.ctm;
                if [t.a, t.b, t.c, t.d, t.tx, t.ty]
                    .iter()
                    .any(|v| !v.is_finite() || v.abs() > 10_000_000.0)
                {
                    return Err("PDF composed transform exceeds coordinate limit".into());
                }
            }
            "G" | "g" | "RG" | "rg" | "K" | "k" | "SC" | "sc" | "SCN" | "scn" => {
                let stroke = operator.as_bytes()[0].is_ascii_uppercase();
                let (space, color) = match operator {
                    "G" | "g" => (
                        PdfDeviceColorSpace::Gray,
                        gray_to_rgb(numbers(&op.operands, 1)?[0]),
                    ),
                    "RG" | "rg" => {
                        let v = numbers(&op.operands, 3)?;
                        (
                            PdfDeviceColorSpace::Rgb,
                            device_rgb_to_rgb(v[0], v[1], v[2]),
                        )
                    }
                    "K" | "k" => {
                        let v = numbers(&op.operands, 4)?;
                        (
                            PdfDeviceColorSpace::Cmyk,
                            device_cmyk_to_rgb(v[0], v[1], v[2], v[3]),
                        )
                    }
                    _ => {
                        let space = if stroke {
                            state.graphics.stroke_space
                        } else {
                            state.graphics.fill_space
                        };
                        let count = match space {
                            PdfDeviceColorSpace::Gray => 1,
                            PdfDeviceColorSpace::Rgb => 3,
                            PdfDeviceColorSpace::Cmyk => 4,
                        };
                        let values = numbers(&op.operands, count)?
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>();
                        (
                            space,
                            color_from_operands(&values, space).ok_or("Invalid PDF color")?,
                        )
                    }
                };
                if stroke {
                    state.graphics.stroke_space = space;
                    state.graphics.stroke_color = color;
                } else {
                    state.graphics.fill_space = space;
                    state.graphics.fill_color = color;
                }
            }
            "CS" | "cs" => {
                let [Object::Name(name)] = op.operands.as_slice() else {
                    return Err("Invalid PDF color space".into());
                };
                let space =
                    parse_device_color_space(&format!("/{}", String::from_utf8_lossy(name)))
                        .ok_or(
                            "PDF non-device color spaces cannot be preserved; convert to RGB first",
                        )?;
                if operator == "CS" {
                    state.graphics.stroke_space = space;
                    state.graphics.stroke_color = default_color_for_space(space);
                } else {
                    state.graphics.fill_space = space;
                    state.graphics.fill_color = default_color_for_space(space);
                }
            }
            "gs" => validate_pdf_graphics_state(doc, resources, &op)?,
            "m" | "l" | "c" | "v" | "y" | "re" | "h" => {
                numbers(
                    &op.operands,
                    match operator {
                        "m" | "l" => 2,
                        "c" => 6,
                        "v" | "y" | "re" => 4,
                        _ => 0,
                    },
                )?;
                path.extend([
                    Operation::new("q", vec![]),
                    cm(state.graphics.ctm),
                    op,
                    Operation::new("Q", vec![]),
                ]);
            }
            "W" | "W*" => {
                numbers(&op.operands, 0)?;
                pending_clip = Some(operator == "W*");
            }
            "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n" => {
                numbers(&op.operands, 0)?;
                if operator != "n" {
                    for painted in path_from_operations(&path, state.graphics, Some(&op))? {
                        append_path(
                            painted,
                            state.clip.as_ref(),
                            operator.ends_with('*'),
                            budget,
                            output,
                        )?;
                    }
                }
                if let Some(evenodd) = pending_clip.take() {
                    let paths = path_from_operations(&path, state.graphics, None)?;
                    let mut rings = MultiPolygon(vec![]);
                    for painted in paths {
                        rings.0.extend(polygons(&painted.path, budget)?.0);
                    }
                    // Normalize the source winding rule before intersecting it
                    // with the already-normalized parent clip.
                    check_clipping_complexity(&[&rings])?;
                    let normalized = rings.union_with_fill_rule(
                        &MultiPolygon(vec![]),
                        if evenodd {
                            FillRule::EvenOdd
                        } else {
                            FillRule::NonZero
                        },
                    );
                    state.clip = Some(match state.clip.take() {
                        Some(old) => {
                            check_clipping_complexity(&[&old, &normalized])?;
                            old.intersection(&normalized)
                        }
                        None => normalized,
                    });
                }
                path.clear();
            }
            "Do" => {
                let [Object::Name(name)] = op.operands.as_slice() else {
                    return Err("PDF Do requires one resource name".into());
                };
                let objects = resources
                    .ok_or("Missing PDF XObject resources")?
                    .as_dict()
                    .and_then(|r| doc.get_dict_in_dict(r, b"XObject"))
                    .map_err(|e| format!("Cannot resolve PDF Do resource: {e}"))?;
                let (id, object) = doc
                    .dereference(objects.get(name).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
                let stream = object.as_stream().map_err(|e| e.to_string())?;
                if stream.dict.has(b"OC") {
                    return Err("PDF optional-content artwork cannot be preserved".into());
                }
                match stream
                    .dict
                    .get(b"Subtype")
                    .and_then(Object::as_name)
                    .map_err(|e| e.to_string())?
                {
                    b"Form" => {
                        let id = id.ok_or("PDF Form must be an indirect object")?;
                        if !active.insert(id) {
                            return Err("PDF contains a cyclic form reference".into());
                        }
                        if stream.dict.has(b"Group")
                            || stream.dict.has(b"Ref")
                            || stream.dict.has(b"PS")
                        {
                            return Err("PDF form compositing or external artwork cannot be preserved; flatten effects first".into());
                        }
                        let mut child = state.clone();
                        if let Ok(value) = stream.dict.get(b"Matrix") {
                            child.graphics.ctm = child
                                .graphics
                                .ctm
                                .compose(&matrix(value.as_array().map_err(|e| e.to_string())?)?);
                        }
                        let t = child.graphics.ctm;
                        if [t.a, t.b, t.c, t.d, t.tx, t.ty]
                            .iter()
                            .any(|v| !v.is_finite() || v.abs() > 10_000_000.0)
                        {
                            return Err(
                                "PDF composed form transform exceeds coordinate limit".into()
                            );
                        }
                        let bbox = numbers(
                            stream
                                .dict
                                .get(b"BBox")
                                .and_then(Object::as_array)
                                .map_err(|e| e.to_string())?,
                            4,
                        )?;
                        if bbox[2] <= bbox[0] || bbox[3] <= bbox[1] {
                            return Err("Invalid PDF form bounding box".into());
                        }
                        let corners = [
                            (bbox[0], bbox[1]),
                            (bbox[2], bbox[1]),
                            (bbox[2], bbox[3]),
                            (bbox[0], bbox[3]),
                            (bbox[0], bbox[1]),
                        ];
                        let ring = LineString(
                            corners
                                .into_iter()
                                .map(|(x, y)| {
                                    let p = child.graphics.ctm.apply(&Point2D::new(x, y));
                                    Coord { x: p.x, y: p.y }
                                })
                                .collect(),
                        );
                        let bounds = MultiPolygon(vec![Polygon::new(ring, vec![])]);
                        child.clip = Some(match child.clip {
                            Some(old) => {
                                check_clipping_complexity(&[&old, &bounds])?;
                                old.intersection(&bounds)
                            }
                            None => bounds,
                        });
                        let resources = match stream.dict.get(b"Resources") {
                            Ok(value) => Some(doc.dereference(value).map_err(|e| e.to_string())?.1),
                            Err(_) => resources,
                        };
                        let decoded = decode_artwork_stream(stream, budget.bytes)?;
                        charge(&mut budget.bytes, decoded.len(), "expanded content")?;
                        walk(doc, resources, &decoded, child, active, budget, output)?;
                        active.remove(&id);
                    }
                    b"Image" => {
                        if let Some(image) = decode_image(stream, &state, budget)? {
                            output.images.push(image);
                        }
                    }
                    _ => return Err("PDF XObject subtype cannot be preserved".into()),
                }
            }
            "d" if matches!(op.operands.as_slice(),[Object::Array(a),p] if a.is_empty() && p.as_float().is_ok_and(|v|v.is_finite() && v>=0.0)) =>
                {}
            "w" | "J" | "j" | "M" => {
                numbers(&op.operands, 1)?;
            }
            _ => {
                return Err(format!(
                    "PDF drawing operator '{operator}' cannot be preserved. Convert text to paths and flatten unsupported effects before importing."
                ));
            }
        }
    }
    if !stack.is_empty() || pending_clip.is_some() {
        return Err("Unbalanced PDF graphics state or unfinished clipping path".into());
    }
    // Retain the legacy path-only fallback, including clipping within Forms.
    for painted in path_from_operations(&path, state.graphics, None)? {
        append_path(painted, state.clip.as_ref(), false, budget, output)?;
    }
    Ok(())
}

fn clip_vertex_count(polygons: &MultiPolygon<f64>) -> usize {
    polygons
        .0
        .iter()
        .map(|p| p.exterior().0.len() + p.interiors().iter().map(|r| r.0.len()).sum::<usize>())
        .sum()
}

fn check_clipping_complexity(inputs: &[&MultiPolygon<f64>]) -> Result<(), String> {
    // Overlay output can grow quadratically for intersecting contours. Bound
    // inputs before allocating it, not only after flattening/clipping finishes.
    if inputs.iter().map(|p| clip_vertex_count(p)).sum::<usize>() > 1024 {
        return Err("PDF clipping complexity limit exceeded".into());
    }
    Ok(())
}

fn polygons(path: &VecPath, budget: &mut Budget) -> Result<MultiPolygon<f64>, String> {
    Ok(MultiPolygon(
        lines(path, budget)?
            .into_iter()
            .filter_map(|mut line| {
                if line.0.len() < 3 {
                    return None;
                }
                if line.0.first() != line.0.last() {
                    line.0.push(line.0[0]);
                }
                Some(Polygon::new(line, vec![]))
            })
            .collect(),
    ))
}

fn line_path(lines: impl IntoIterator<Item = LineString<f64>>, closed: bool) -> VecPath {
    VecPath {
        subpaths: lines
            .into_iter()
            .filter_map(|line| {
                let mut coords = line.0.into_iter();
                let start = coords.next()?;
                let mut commands = vec![PathCommand::MoveTo {
                    x: start.x,
                    y: start.y,
                }];
                commands.extend(coords.map(|p| PathCommand::LineTo { x: p.x, y: p.y }));
                if closed {
                    commands.push(PathCommand::Close);
                }
                Some(SubPath { commands, closed })
            })
            .collect(),
    }
}

fn append_path(
    mut painted: PdfPaintedPath,
    clip: Option<&MultiPolygon<f64>>,
    evenodd: bool,
    budget: &mut Budget,
    output: &mut PdfArtwork,
) -> Result<(), String> {
    let Some(clip) = clip else {
        output.paths.push(painted);
        return Ok(());
    };
    if clip.0.is_empty() {
        return Ok(());
    }
    // Preserve exact Beziers when the whole bounds lies inside the clip.
    if painted.path.visual_bounds().is_some_and(|b| {
        clip.contains(
            &geo::Rect::new(
                Coord {
                    x: b.min.x,
                    y: b.min.y,
                },
                Coord {
                    x: b.max.x,
                    y: b.max.y,
                },
            )
            .to_polygon(),
        )
    }) {
        output.paths.push(painted);
        return Ok(());
    }
    if matches!(painted.paint_mode, PdfPaintMode::FillStroke) {
        let mut stroke = painted.clone();
        stroke.paint_mode = PdfPaintMode::Stroke;
        stroke.fill_color = None;
        append_path(stroke, Some(clip), evenodd, budget, output)?;
        painted.paint_mode = PdfPaintMode::Fill;
        painted.stroke_color = None;
    }
    painted.path = if painted.paint_mode == PdfPaintMode::Fill {
        let subject = polygons(&painted.path, budget)?;
        check_clipping_complexity(&[&subject, clip])?;
        let clipped = subject.intersection_with_fill_rule(
            clip,
            if evenodd {
                FillRule::EvenOdd
            } else {
                FillRule::NonZero
            },
        );
        line_path(
            clipped.0.into_iter().flat_map(|polygon| {
                let (outer, inner) = polygon.into_inner();
                std::iter::once(outer).chain(inner)
            }),
            true,
        )
    } else {
        let lines = MultiLineString(lines(&painted.path, budget)?);
        let segments = lines.0.iter().map(|l| l.0.len()).sum::<usize>();
        if segments.saturating_mul(clip_vertex_count(clip)) > 1_000_000 {
            return Err("PDF clipping complexity limit exceeded".into());
        }
        line_path(clip.clip(&lines, false).0, false)
    };
    charge(
        &mut budget.points,
        painted.path.command_count(),
        "clipped output geometry",
    )?;
    if !painted.path.is_empty() {
        output.paths.push(painted);
    }
    Ok(())
}

// Bound subdivisions as they are produced, rather than checking after a large
// flattened curve has already allocated all of its points.
fn lines(path: &VecPath, budget: &mut Budget) -> Result<Vec<LineString<f64>>, String> {
    let mut result = Vec::new();
    for sub in &path.subpaths {
        let mut line = Vec::new();
        let mut at = Point2D::zero();
        let mut start = at;
        for command in &sub.commands {
            match *command {
                PathCommand::MoveTo { x, y } => {
                    if line.len() > 1 {
                        result.push(LineString(std::mem::take(&mut line)));
                    }
                    at = Point2D::new(x, y);
                    start = at;
                    line.push(Coord { x, y });
                }
                PathCommand::LineTo { x, y } => {
                    at = Point2D::new(x, y);
                    line.push(Coord { x, y });
                }
                PathCommand::Close => {
                    at = start;
                    line.push(Coord { x: at.x, y: at.y });
                }
                PathCommand::QuadTo { cx, cy, x, y } => {
                    let end = Point2D::new(x, y);
                    let control = Point2D::new(cx, cy);
                    cubic(
                        at,
                        Point2D::new(
                            at.x + (control.x - at.x) * 2.0 / 3.0,
                            at.y + (control.y - at.y) * 2.0 / 3.0,
                        ),
                        Point2D::new(
                            end.x + (control.x - end.x) * 2.0 / 3.0,
                            end.y + (control.y - end.y) * 2.0 / 3.0,
                        ),
                        end,
                        &mut line,
                        budget,
                    )?;
                    at = end;
                }
                PathCommand::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                } => {
                    let end = Point2D::new(x, y);
                    cubic(
                        at,
                        Point2D::new(c1x, c1y),
                        Point2D::new(c2x, c2y),
                        end,
                        &mut line,
                        budget,
                    )?;
                    at = end;
                }
            }
            charge(&mut budget.points, 1, "clipping geometry")?;
        }
        if line.len() > 1 {
            result.push(LineString(line));
        }
    }
    Ok(result)
}
fn cubic(
    a: Point2D,
    b: Point2D,
    c: Point2D,
    d: Point2D,
    out: &mut Vec<Coord<f64>>,
    budget: &mut Budget,
) -> Result<(), String> {
    let mid = |a: Point2D, b: Point2D| Point2D::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let mut stack = vec![(a, b, c, d, 0)];
    while let Some((a, b, c, d, depth)) = stack.pop() {
        let distance = |p: Point2D| {
            let dx = d.x - a.x;
            let dy = d.y - a.y;
            let len = dx.hypot(dy);
            if len < 1e-12 {
                (p.x - a.x).hypot(p.y - a.y)
            } else {
                ((p.x - a.x) * dy - (p.y - a.y) * dx).abs() / len
            }
        };
        if distance(b).max(distance(c)) < 0.01 / PT_TO_MM
            && (b.x - a.x).hypot(b.y - a.y)
                + (c.x - b.x).hypot(c.y - b.y)
                + (d.x - c.x).hypot(d.y - c.y)
                - (d.x - a.x).hypot(d.y - a.y)
                < 0.01 / PT_TO_MM
        {
            charge(&mut budget.points, 1, "clipping geometry")?;
            out.push(Coord { x: d.x, y: d.y });
            continue;
        }
        if depth >= 20 {
            return Err("PDF clipping curve subdivision limit exceeded".into());
        }
        let ab = mid(a, b);
        let bc = mid(b, c);
        let cd = mid(c, d);
        let abc = mid(ab, bc);
        let bcd = mid(bc, cd);
        let center = mid(abc, bcd);
        stack.push((center, bcd, cd, d, depth + 1));
        stack.push((a, ab, abc, center, depth + 1));
    }
    Ok(())
}

mod images;
use images::decode_image;

use beambench_common::geometry::{Bounds, Point2D, Transform2D};
use beambench_common::path::{PathCommand, SubPath, VecPath};

use crate::Project;
use crate::object::{ObjectData, ProjectObject, ShapeKind};
use crate::vector::text_to_path::{text_object_local_path, text_object_local_path_with_bounds};
use crate::vector::transform::bake_transform;

/// Convert a Shape to a VecPath.
pub fn shape_to_vecpath(kind: ShapeKind, width: f64, height: f64, corner_radius: f64) -> VecPath {
    match kind {
        ShapeKind::Rectangle => rect_to_vecpath(width, height, corner_radius),
        ShapeKind::Ellipse => ellipse_to_vecpath(width, height),
    }
}

/// Convert any ObjectData to a VecPath (if possible).
/// Returns None for RasterImage.
pub fn object_to_vecpath(data: &ObjectData) -> Option<VecPath> {
    match data {
        ObjectData::Shape {
            kind,
            width,
            height,
            corner_radius,
        } => Some(shape_to_vecpath(*kind, *width, *height, *corner_radius)),
        ObjectData::Star {
            points,
            bulge,
            ratio,
            dual_radius,
            ratio2,
            corner_radius,
            corner_radii,
        } => Some(star_to_vecpath(
            *points,
            *bulge,
            *ratio,
            *dual_radius,
            *ratio2,
            *corner_radius,
            corner_radii,
        )),
        ObjectData::VectorPath { path_data, .. } => {
            let parsed = VecPath::parse_svg_d(path_data);
            if let Some(bbox) = parsed.visual_bounds() {
                if bbox.min.x.abs() > 1e-9 || bbox.min.y.abs() > 1e-9 {
                    let normalize = Transform2D::translate(-bbox.min.x, -bbox.min.y);
                    Some(bake_transform(&parsed, &normalize))
                } else {
                    Some(parsed)
                }
            } else {
                Some(parsed)
            }
        }
        ObjectData::Text { .. } => text_object_local_path(data),
        ObjectData::Polygon { sides, radius } => Some(polygon_to_vecpath(*sides, *radius)),
        ObjectData::Barcode {
            barcode_type,
            data,
            width,
            height,
            options,
        } => crate::barcode_gen::generate_barcode_with_options(
            *barcode_type,
            data,
            *width,
            *height,
            options,
        )
        .ok(),
        ObjectData::RasterImage { .. } => None,
        ObjectData::Group { .. } => None,
        ObjectData::VirtualClone { .. } => None,
    }
}

/// Convert a project object into a world-space VecPath that matches its current
/// displayed bounds and transform.
pub fn object_to_world_vecpath(obj: &ProjectObject) -> Option<VecPath> {
    let world = if matches!(obj.data, ObjectData::Text { .. }) {
        let path = text_object_local_path_with_bounds(&obj.data, &obj.bounds)
            .or_else(|| object_to_vecpath(&obj.data))?;
        let translated = bake_transform(
            &path,
            &Transform2D::translate(obj.bounds.min.x, obj.bounds.min.y),
        );
        if !obj.transform.is_identity() {
            bake_transform_around_bounds_center(&translated, &obj.transform, &obj.bounds)
        } else {
            translated
        }
    } else {
        let path = object_to_vecpath(&obj.data)?;

        // Step 1: map local path geometry to object bounds
        let intrinsic = path.visual_bounds().or_else(|| path.bounds())?;
        let old_w = intrinsic.max.x - intrinsic.min.x;
        let old_h = intrinsic.max.y - intrinsic.min.y;
        let new_w = obj.bounds.max.x - obj.bounds.min.x;
        let new_h = obj.bounds.max.y - obj.bounds.min.y;

        let sx = if old_w > 0.0 { new_w / old_w } else { 1.0 };
        let sy = if old_h > 0.0 { new_h / old_h } else { 1.0 };
        let tx = obj.bounds.min.x - intrinsic.min.x * sx;
        let ty = obj.bounds.min.y - intrinsic.min.y * sy;

        let needs_mapping = (sx - 1.0).abs() > 1e-9
            || (sy - 1.0).abs() > 1e-9
            || tx.abs() > 1e-9
            || ty.abs() > 1e-9;

        let mapped = if needs_mapping {
            bake_transform(
                &path,
                &Transform2D {
                    a: sx,
                    b: 0.0,
                    c: 0.0,
                    d: sy,
                    tx,
                    ty,
                },
            )
        } else {
            path
        };

        // Step 2: apply object transform (translate, rotate, etc.) on top
        if !obj.transform.is_identity() {
            bake_transform_around_bounds_center(&mapped, &obj.transform, &obj.bounds)
        } else {
            mapped
        }
    };

    // For non-VectorPath objects, start_point_edits are stored as metadata
    // (not baked into path_data). Apply them lazily so that downstream
    // consumers (planner, export, canvas) see the edited start point.
    if !matches!(obj.data, ObjectData::VectorPath { .. }) && !obj.start_point_edits.is_empty() {
        Some(crate::vector::path_ops::apply_start_point_edits_forward(
            &world,
            &obj.start_point_edits,
        ))
    } else {
        Some(world)
    }
}

fn bake_transform_around_bounds_center(
    path: &VecPath,
    transform: &Transform2D,
    bounds: &Bounds,
) -> VecPath {
    let cx = (bounds.min.x + bounds.max.x) / 2.0;
    let cy = (bounds.min.y + bounds.max.y) / 2.0;
    let around_center = Transform2D::translate(cx, cy)
        .compose(transform)
        .compose(&Transform2D::translate(-cx, -cy));
    bake_transform(path, &around_center)
}

/// Like `object_to_world_vecpath`, but resolves VirtualClones first by
/// copying the source's data into the clone's bounds/transform before
/// computing the world-space VecPath.
pub fn object_to_world_vecpath_resolved(obj: &ProjectObject, project: &Project) -> Option<VecPath> {
    if matches!(obj.data, ObjectData::VirtualClone { .. }) {
        let resolved = project.resolve_clone(obj)?;
        object_to_world_vecpath(&resolved)
    } else {
        object_to_world_vecpath(obj)
    }
}

pub fn object_local_point_to_world(obj: &ProjectObject, local: Point2D) -> Option<Point2D> {
    if matches!(obj.data, ObjectData::Text { .. }) {
        let translated = Point2D::new(local.x + obj.bounds.min.x, local.y + obj.bounds.min.y);
        return Some(if !obj.transform.is_identity() {
            obj.transform.apply_around_center(
                &translated,
                &Point2D::new(
                    (obj.bounds.min.x + obj.bounds.max.x) / 2.0,
                    (obj.bounds.min.y + obj.bounds.max.y) / 2.0,
                ),
            )
        } else {
            translated
        });
    }

    let path = object_to_vecpath(&obj.data)?;
    let intrinsic = path.visual_bounds().or_else(|| path.bounds())?;
    let old_w = intrinsic.max.x - intrinsic.min.x;
    let old_h = intrinsic.max.y - intrinsic.min.y;
    let new_w = obj.bounds.max.x - obj.bounds.min.x;
    let new_h = obj.bounds.max.y - obj.bounds.min.y;

    let sx = if old_w > 0.0 { new_w / old_w } else { 1.0 };
    let sy = if old_h > 0.0 { new_h / old_h } else { 1.0 };
    let mapped = Point2D::new(
        obj.bounds.min.x + (local.x - intrinsic.min.x) * sx,
        obj.bounds.min.y + (local.y - intrinsic.min.y) * sy,
    );
    Some(if !obj.transform.is_identity() {
        obj.transform.apply_around_center(
            &mapped,
            &Point2D::new(
                (obj.bounds.min.x + obj.bounds.max.x) / 2.0,
                (obj.bounds.min.y + obj.bounds.max.y) / 2.0,
            ),
        )
    } else {
        mapped
    })
}

/// Rectangle to VecPath: four LineTo commands + Close.
fn rect_to_vecpath(width: f64, height: f64, corner_radius: f64) -> VecPath {
    if corner_radius > 0.0 {
        rounded_rect_to_vecpath(width, height, corner_radius)
    } else if corner_radius < 0.0 {
        bitten_rect_to_vecpath(width, height, corner_radius.abs())
    } else {
        let mut sp = SubPath::new();
        sp.commands.push(PathCommand::MoveTo { x: 0.0, y: 0.0 });
        sp.commands.push(PathCommand::LineTo { x: width, y: 0.0 });
        sp.commands.push(PathCommand::LineTo {
            x: width,
            y: height,
        });
        sp.commands.push(PathCommand::LineTo { x: 0.0, y: height });
        sp.commands.push(PathCommand::Close);
        sp.closed = true;
        VecPath { subpaths: vec![sp] }
    }
}

/// Rounded rectangle using cubic bezier corners.
fn rounded_rect_to_vecpath(width: f64, height: f64, r: f64) -> VecPath {
    let r = r.min(width / 2.0).min(height / 2.0);
    // Cubic bezier approximation of quarter circle: control point offset
    let k = r * 0.5522847498;

    let mut sp = SubPath::new();
    // Start at top-left corner, after the rounded part
    sp.commands.push(PathCommand::MoveTo { x: r, y: 0.0 });

    // Top edge → top-right corner
    sp.commands.push(PathCommand::LineTo {
        x: width - r,
        y: 0.0,
    });
    sp.commands.push(PathCommand::CubicTo {
        c1x: width - r + k,
        c1y: 0.0,
        c2x: width,
        c2y: k,
        x: width,
        y: r,
    });

    // Right edge → bottom-right corner
    sp.commands.push(PathCommand::LineTo {
        x: width,
        y: height - r,
    });
    sp.commands.push(PathCommand::CubicTo {
        c1x: width,
        c1y: height - r + k,
        c2x: width - r + k,
        c2y: height,
        x: width - r,
        y: height,
    });

    // Bottom edge → bottom-left corner
    sp.commands.push(PathCommand::LineTo { x: r, y: height });
    sp.commands.push(PathCommand::CubicTo {
        c1x: r - k,
        c1y: height,
        c2x: 0.0,
        c2y: height - r + k,
        x: 0.0,
        y: height - r,
    });

    // Left edge → top-left corner
    sp.commands.push(PathCommand::LineTo { x: 0.0, y: r });
    sp.commands.push(PathCommand::CubicTo {
        c1x: 0.0,
        c1y: r - k,
        c2x: r - k,
        c2y: 0.0,
        x: r,
        y: 0.0,
    });

    sp.commands.push(PathCommand::Close);
    sp.closed = true;
    VecPath { subpaths: vec![sp] }
}

/// Negative rectangle corner radius: quarter-circle bites centered on the
/// rectangle corners. This intentionally behaves differently from the generic
/// radius tool and matches the built-in rectangle control model.
fn bitten_rect_to_vecpath(width: f64, height: f64, r: f64) -> VecPath {
    let r = r.min(width / 2.0).min(height / 2.0);
    let k = r * 0.5522847498;

    let mut sp = SubPath::new();
    sp.commands.push(PathCommand::MoveTo { x: r, y: 0.0 });

    sp.commands.push(PathCommand::LineTo {
        x: width - r,
        y: 0.0,
    });
    sp.commands.push(PathCommand::CubicTo {
        c1x: width - r,
        c1y: k,
        c2x: width - k,
        c2y: r,
        x: width,
        y: r,
    });

    sp.commands.push(PathCommand::LineTo {
        x: width,
        y: height - r,
    });
    sp.commands.push(PathCommand::CubicTo {
        c1x: width - k,
        c1y: height - r,
        c2x: width - r,
        c2y: height - k,
        x: width - r,
        y: height,
    });

    sp.commands.push(PathCommand::LineTo { x: r, y: height });
    sp.commands.push(PathCommand::CubicTo {
        c1x: r,
        c1y: height - k,
        c2x: k,
        c2y: height - r,
        x: 0.0,
        y: height - r,
    });

    sp.commands.push(PathCommand::LineTo { x: 0.0, y: r });
    sp.commands.push(PathCommand::CubicTo {
        c1x: k,
        c1y: r,
        c2x: r,
        c2y: k,
        x: r,
        y: 0.0,
    });

    sp.commands.push(PathCommand::Close);
    sp.closed = true;
    VecPath { subpaths: vec![sp] }
}

fn polygon_to_vecpath(sides: u32, radius: f64) -> VecPath {
    let sides = sides.max(3);
    let mut sp = SubPath::new();
    for i in 0..sides {
        let theta = (i as f64 / sides as f64) * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
        let x = radius + radius * theta.cos();
        let y = radius + radius * theta.sin();
        if i == 0 {
            sp.commands.push(PathCommand::MoveTo { x, y });
        } else {
            sp.commands.push(PathCommand::LineTo { x, y });
        }
    }
    sp.commands.push(PathCommand::Close);
    sp.closed = true;
    VecPath { subpaths: vec![sp] }
}

pub fn star_anchor_points(
    points: u32,
    ratio: f64,
    dual_radius: bool,
    ratio2: Option<f64>,
) -> Vec<Point2D> {
    let points = points.max(3);
    let outer_radius = 50.0;
    let ratio_a = ratio.clamp(0.05, 0.95);
    let ratio_b = ratio2.unwrap_or(0.7).clamp(0.05, 1.0);
    let cx = outer_radius;
    let cy = outer_radius;

    if dual_radius {
        let step = std::f64::consts::TAU / points as f64;
        let valley_r = outer_radius * ratio_a;
        let secondary_r = outer_radius * ratio_b;
        let mut pts = Vec::with_capacity(4 * points as usize);
        for i in 0..points {
            let base_angle = i as f64 * step - std::f64::consts::FRAC_PI_2;
            pts.push(star_anchor_point(cx, cy, outer_radius, base_angle));
            pts.push(star_anchor_point(
                cx,
                cy,
                valley_r,
                base_angle + step * 0.25,
            ));
            pts.push(star_anchor_point(
                cx,
                cy,
                secondary_r,
                base_angle + step * 0.5,
            ));
            pts.push(star_anchor_point(
                cx,
                cy,
                valley_r,
                base_angle + step * 0.75,
            ));
        }
        pts
    } else {
        let outer_step = std::f64::consts::TAU / points as f64;
        let inner_radius = outer_radius * ratio_a;
        let mut pts = Vec::with_capacity(2 * points as usize);
        for i in 0..points {
            let outer_angle = i as f64 * outer_step - std::f64::consts::FRAC_PI_2;
            let inner_angle = outer_angle + outer_step * 0.5;
            pts.push(star_anchor_point(cx, cy, outer_radius, outer_angle));
            pts.push(star_anchor_point(cx, cy, inner_radius, inner_angle));
        }
        pts
    }
}

pub fn star_display_points(
    points: u32,
    ratio: f64,
    dual_radius: bool,
    ratio2: Option<f64>,
    corner_radius: f64,
    corner_radii: &[f64],
) -> Vec<Point2D> {
    let anchors = star_anchor_points(points, ratio, dual_radius, ratio2);
    let effective_corner_radii =
        star_effective_corner_radii(anchors.len(), corner_radius, corner_radii);
    if !effective_corner_radii.iter().any(|r| *r > 1e-9) {
        return anchors;
    }
    build_rounded_star_corners(&anchors, &effective_corner_radii)
        .into_iter()
        .enumerate()
        .map(|(idx, corner)| {
            if corner.rounded {
                cubic_closest_point_to_target(
                    corner.entry,
                    corner.c1,
                    corner.c2,
                    corner.exit,
                    anchors[idx],
                )
            } else {
                anchors[idx]
            }
        })
        .collect()
}

fn star_to_vecpath(
    points: u32,
    bulge: f64,
    ratio: f64,
    dual_radius: bool,
    ratio2: Option<f64>,
    corner_radius: f64,
    corner_radii: &[f64],
) -> VecPath {
    let outer_radius = 50.0;
    let bulge_factor = bulge.clamp(0.0, 1.0);
    let anchor_points = star_anchor_points(points, ratio, dual_radius, ratio2);
    let effective_corner_radii =
        star_effective_corner_radii(anchor_points.len(), corner_radius, corner_radii);

    // Collect all anchors first, then emit commands (possibly with curves).
    // Each anchor is (x, y, radius) in local space centered at (outer_radius, outer_radius).
    let verts: Vec<(f64, f64, f64)> = anchor_points
        .iter()
        .map(|pt| {
            let dx = pt.x - outer_radius;
            let dy = pt.y - outer_radius;
            (pt.x, pt.y, (dx * dx + dy * dy).sqrt())
        })
        .collect();

    if bulge_factor.abs() < 1e-9 && effective_corner_radii.iter().any(|r| *r > 1e-9) {
        return rounded_star_to_vecpath(&anchor_points, &effective_corner_radii);
    }

    // Emit path commands — straight LineTo when bulge==0, CubicTo when bulge!=0.
    let mut sp = SubPath::new();
    let n = verts.len();
    for i in 0..n {
        let (ax, ay, _) = verts[i];
        if i == 0 {
            sp.commands.push(PathCommand::MoveTo { x: ax, y: ay });
        } else if bulge_factor.abs() < 1e-9 {
            sp.commands.push(PathCommand::LineTo { x: ax, y: ay });
        } else {
            let (px, py, pr) = verts[i - 1];
            let (_, _, ar) = verts[i];
            let (qx, qy) = bulge_control_point(
                outer_radius,
                outer_radius,
                px,
                py,
                pr,
                ax,
                ay,
                ar,
                bulge_factor,
            );
            // Quadratic→Cubic: C1 = P + 2/3*(Q-P), C2 = A + 2/3*(Q-A)
            let c1x = px + 2.0 / 3.0 * (qx - px);
            let c1y = py + 2.0 / 3.0 * (qy - py);
            let c2x = ax + 2.0 / 3.0 * (qx - ax);
            let c2y = ay + 2.0 / 3.0 * (qy - ay);
            sp.commands.push(PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x: ax,
                y: ay,
            });
        }
    }
    // Closing edge: if bulge, emit a CubicTo back to the first point
    if bulge_factor.abs() >= 1e-9 && n > 1 {
        let (px, py, pr) = verts[n - 1];
        let (ax, ay, ar) = verts[0];
        let (qx, qy) = bulge_control_point(
            outer_radius,
            outer_radius,
            px,
            py,
            pr,
            ax,
            ay,
            ar,
            bulge_factor,
        );
        let c1x = px + 2.0 / 3.0 * (qx - px);
        let c1y = py + 2.0 / 3.0 * (qy - py);
        let c2x = ax + 2.0 / 3.0 * (qx - ax);
        let c2y = ay + 2.0 / 3.0 * (qy - ay);
        sp.commands.push(PathCommand::CubicTo {
            c1x,
            c1y,
            c2x,
            c2y,
            x: ax,
            y: ay,
        });
    }
    sp.commands.push(PathCommand::Close);
    sp.closed = true;
    VecPath { subpaths: vec![sp] }
}

fn star_anchor_point(cx: f64, cy: f64, radius: f64, angle: f64) -> Point2D {
    Point2D::new(cx + radius * angle.cos(), cy + radius * angle.sin())
}

fn star_effective_corner_radii(
    anchor_count: usize,
    corner_radius: f64,
    corner_radii: &[f64],
) -> Vec<f64> {
    if corner_radii.len() == anchor_count {
        corner_radii.iter().map(|r| r.max(0.0)).collect()
    } else {
        vec![corner_radius.max(0.0); anchor_count]
    }
}

#[derive(Debug, Clone, Copy)]
struct RoundedCorner {
    entry: Point2D,
    exit: Point2D,
    c1: Point2D,
    c2: Point2D,
    rounded: bool,
}

fn rounded_star_to_vecpath(anchors: &[Point2D], radii: &[f64]) -> VecPath {
    let mut sp = SubPath::new();
    if anchors.len() < 3 {
        return VecPath { subpaths: vec![sp] };
    }
    let corners = build_rounded_star_corners(anchors, radii);
    let first = corners[0];
    sp.commands.push(PathCommand::MoveTo {
        x: if first.rounded {
            first.exit.x
        } else {
            anchors[0].x
        },
        y: if first.rounded {
            first.exit.y
        } else {
            anchors[0].y
        },
    });
    for i in 1..anchors.len() {
        let corner = corners[i];
        if corner.rounded {
            sp.commands.push(PathCommand::LineTo {
                x: corner.entry.x,
                y: corner.entry.y,
            });
            sp.commands.push(PathCommand::CubicTo {
                c1x: corner.c1.x,
                c1y: corner.c1.y,
                c2x: corner.c2.x,
                c2y: corner.c2.y,
                x: corner.exit.x,
                y: corner.exit.y,
            });
        } else {
            sp.commands.push(PathCommand::LineTo {
                x: anchors[i].x,
                y: anchors[i].y,
            });
        }
    }
    if first.rounded {
        sp.commands.push(PathCommand::LineTo {
            x: first.entry.x,
            y: first.entry.y,
        });
        sp.commands.push(PathCommand::CubicTo {
            c1x: first.c1.x,
            c1y: first.c1.y,
            c2x: first.c2.x,
            c2y: first.c2.y,
            x: first.exit.x,
            y: first.exit.y,
        });
    }
    sp.commands.push(PathCommand::Close);
    sp.closed = true;
    VecPath { subpaths: vec![sp] }
}

fn build_rounded_star_corners(anchors: &[Point2D], radii: &[f64]) -> Vec<RoundedCorner> {
    let n = anchors.len();
    let mut result = Vec::with_capacity(n);
    for i in 0..n {
        let prev = anchors[(i + n - 1) % n];
        let curr = anchors[i];
        let next = anchors[(i + 1) % n];
        let radius = radii.get(i).copied().unwrap_or(0.0).max(0.0);
        let incoming = Point2D::new(curr.x - prev.x, curr.y - prev.y);
        let outgoing = Point2D::new(next.x - curr.x, next.y - curr.y);
        let len_in = (incoming.x * incoming.x + incoming.y * incoming.y).sqrt();
        let len_out = (outgoing.x * outgoing.x + outgoing.y * outgoing.y).sqrt();
        if radius < 1e-9 || len_in < 1e-9 || len_out < 1e-9 {
            result.push(RoundedCorner {
                entry: curr,
                exit: curr,
                c1: curr,
                c2: curr,
                rounded: false,
            });
            continue;
        }
        let trim = radius.min(len_in * 0.45).min(len_out * 0.45);
        if trim < 1e-9 {
            result.push(RoundedCorner {
                entry: curr,
                exit: curr,
                c1: curr,
                c2: curr,
                rounded: false,
            });
            continue;
        }
        let in_unit = Point2D::new(incoming.x / len_in, incoming.y / len_in);
        let out_unit = Point2D::new(outgoing.x / len_out, outgoing.y / len_out);
        let entry = Point2D::new(curr.x - in_unit.x * trim, curr.y - in_unit.y * trim);
        let exit = Point2D::new(curr.x + out_unit.x * trim, curr.y + out_unit.y * trim);
        let handle = trim * 0.552_284_749_8;
        let control = Point2D::new(
            (entry.x + exit.x) * 0.5 + (in_unit.x - out_unit.x) * handle * 0.25,
            (entry.y + exit.y) * 0.5 + (in_unit.y - out_unit.y) * handle * 0.25,
        );
        let c1 = Point2D::new(
            entry.x + (control.x - entry.x) * (2.0 / 3.0),
            entry.y + (control.y - entry.y) * (2.0 / 3.0),
        );
        let c2 = Point2D::new(
            exit.x + (control.x - exit.x) * (2.0 / 3.0),
            exit.y + (control.y - exit.y) * (2.0 / 3.0),
        );
        result.push(RoundedCorner {
            entry,
            exit,
            c1,
            c2,
            rounded: true,
        });
    }
    result
}

fn cubic_point(p0: Point2D, c1: Point2D, c2: Point2D, p1: Point2D, t: f64) -> Point2D {
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let t2 = t * t;
    Point2D::new(
        mt2 * mt * p0.x + 3.0 * mt2 * t * c1.x + 3.0 * mt * t2 * c2.x + t2 * t * p1.x,
        mt2 * mt * p0.y + 3.0 * mt2 * t * c1.y + 3.0 * mt * t2 * c2.y + t2 * t * p1.y,
    )
}

fn cubic_closest_point_to_target(
    p0: Point2D,
    c1: Point2D,
    c2: Point2D,
    p1: Point2D,
    target: Point2D,
) -> Point2D {
    let mut best_t = 0.5;
    let mut best_d2 = f64::INFINITY;

    // Candidate markers should sit on the visible rounded corner near the
    // original sharp anchor, not at an arbitrary midpoint on the curve.
    for step in 0..=24 {
        let t = step as f64 / 24.0;
        let pt = cubic_point(p0, c1, c2, p1, t);
        let d2 = (pt.x - target.x).powi(2) + (pt.y - target.y).powi(2);
        if d2 < best_d2 {
            best_d2 = d2;
            best_t = t;
        }
    }

    cubic_point(p0, c1, c2, p1, best_t)
}

/// Compute a quadratic control point for a bulged star edge.
/// The control point stays on the radial line through the edge midpoint so the
/// anchor angles stay fixed and only the flank curvature changes.
fn bulge_control_point(
    cx: f64,
    cy: f64,
    ax: f64,
    ay: f64,
    ar: f64,
    bx: f64,
    by: f64,
    br: f64,
    bulge: f64,
) -> (f64, f64) {
    let mx = (ax + bx) * 0.5;
    let my = (ay + by) * 0.5;
    let dx = mx - cx;
    let dy = my - cy;
    let mid_radius = (dx * dx + dy * dy).sqrt();
    if mid_radius < 1e-12 {
        return (mx, my);
    }
    let target_radius = mid_radius + bulge * (ar.max(br) - mid_radius);
    (
        cx + dx / mid_radius * target_radius,
        cy + dy / mid_radius * target_radius,
    )
}

/// Ellipse to VecPath: four cubic bezier arcs approximating a full ellipse.
fn ellipse_to_vecpath(width: f64, height: f64) -> VecPath {
    let rx = width / 2.0;
    let ry = height / 2.0;
    let cx = rx;
    let cy = ry;

    // Cubic bezier approximation of quarter circle: control point offset ratio
    let kx = rx * 0.5522847498;
    let ky = ry * 0.5522847498;

    let mut sp = SubPath::new();

    // Start at top center
    sp.commands.push(PathCommand::MoveTo { x: cx, y: cy - ry });

    // Top-right quadrant
    sp.commands.push(PathCommand::CubicTo {
        c1x: cx + kx,
        c1y: cy - ry,
        c2x: cx + rx,
        c2y: cy - ky,
        x: cx + rx,
        y: cy,
    });

    // Bottom-right quadrant
    sp.commands.push(PathCommand::CubicTo {
        c1x: cx + rx,
        c1y: cy + ky,
        c2x: cx + kx,
        c2y: cy + ry,
        x: cx,
        y: cy + ry,
    });

    // Bottom-left quadrant
    sp.commands.push(PathCommand::CubicTo {
        c1x: cx - kx,
        c1y: cy + ry,
        c2x: cx - rx,
        c2y: cy + ky,
        x: cx - rx,
        y: cy,
    });

    // Top-left quadrant
    sp.commands.push(PathCommand::CubicTo {
        c1x: cx - rx,
        c1y: cy - ky,
        c2x: cx - kx,
        c2y: cy - ry,
        x: cx,
        y: cy - ry,
    });

    sp.commands.push(PathCommand::Close);
    sp.closed = true;
    VecPath { subpaths: vec![sp] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::{Bounds, Point2D};

    #[test]
    fn rect_has_four_lines_and_close() {
        let path = shape_to_vecpath(ShapeKind::Rectangle, 100.0, 50.0, 0.0);
        assert_eq!(path.subpaths.len(), 1);
        assert!(path.subpaths[0].closed);
        // MoveTo + 3 LineTo + Close = 5 commands
        assert_eq!(path.subpaths[0].commands.len(), 5);
    }

    #[test]
    fn rect_corner_coordinates() {
        let path = shape_to_vecpath(ShapeKind::Rectangle, 100.0, 50.0, 0.0);
        let cmds = &path.subpaths[0].commands;
        assert_eq!(cmds[0], PathCommand::MoveTo { x: 0.0, y: 0.0 });
        assert_eq!(cmds[1], PathCommand::LineTo { x: 100.0, y: 0.0 });
        assert_eq!(cmds[2], PathCommand::LineTo { x: 100.0, y: 50.0 });
        assert_eq!(cmds[3], PathCommand::LineTo { x: 0.0, y: 50.0 });
    }

    #[test]
    fn ellipse_has_four_cubics_and_close() {
        let path = shape_to_vecpath(ShapeKind::Ellipse, 100.0, 80.0, 0.0);
        assert_eq!(path.subpaths.len(), 1);
        assert!(path.subpaths[0].closed);
        // MoveTo + 4 CubicTo + Close = 6 commands
        assert_eq!(path.subpaths[0].commands.len(), 6);
    }

    #[test]
    fn ellipse_starts_at_top_center() {
        let path = shape_to_vecpath(ShapeKind::Ellipse, 100.0, 80.0, 0.0);
        let first = path.subpaths[0].commands[0];
        assert_eq!(first, PathCommand::MoveTo { x: 50.0, y: 0.0 });
    }

    #[test]
    fn object_to_vecpath_dispatches_shape() {
        let data = ObjectData::Shape {
            kind: ShapeKind::Rectangle,
            width: 10.0,
            height: 20.0,
            corner_radius: 0.0,
        };
        let path = object_to_vecpath(&data).unwrap();
        assert!(!path.is_empty());
    }

    #[test]
    fn object_to_vecpath_dispatches_vector_path() {
        let data = ObjectData::VectorPath {
            path_data: "M0 0 L10 10".to_string(),
            closed: false,
            ruler_guide_axis: None,
        };
        let path = object_to_vecpath(&data).unwrap();
        assert_eq!(path.subpaths[0].commands.len(), 2);
    }

    #[test]
    fn object_to_vecpath_returns_none_for_raster() {
        let data = ObjectData::RasterImage {
            asset_key: "test".to_string(),
            original_width_px: 100,
            original_height_px: 100,
            adjustments: None,
            masks: Vec::new(),
        };
        assert!(object_to_vecpath(&data).is_none());
    }

    #[test]
    fn object_to_vecpath_normalizes_vector_path_to_origin() {
        let data = ObjectData::VectorPath {
            path_data: "M50 100 L150 200".to_string(),
            closed: false,
            ruler_guide_axis: None,
        };
        let path = object_to_vecpath(&data).unwrap();
        let bounds = path.bounds().unwrap();
        assert!(bounds.min.x.abs() < 0.001);
        assert!(bounds.min.y.abs() < 0.001);
        assert!((bounds.max.x - 100.0).abs() < 0.001);
        assert!((bounds.max.y - 100.0).abs() < 0.001);
    }

    #[test]
    fn object_to_world_vecpath_scales_polygon_to_object_bounds() {
        let obj = ProjectObject::new(
            "hex",
            crate::object::LayerRef::new(),
            Bounds::new(Point2D::new(10.0, 20.0), Point2D::new(90.0, 50.0)),
            ObjectData::Polygon {
                sides: 6,
                radius: 20.0,
            },
        );

        let path = object_to_world_vecpath(&obj).unwrap();
        let bounds = path.bounds().unwrap();
        assert!((bounds.min.x - 10.0).abs() < 0.001);
        assert!((bounds.min.y - 20.0).abs() < 0.001);
        assert!((bounds.max.x - 90.0).abs() < 0.001);
        assert!((bounds.max.y - 50.0).abs() < 0.001);
    }

    #[test]
    fn rounded_rect_has_curves() {
        let path = shape_to_vecpath(ShapeKind::Rectangle, 100.0, 50.0, 5.0);
        let has_cubic = path.subpaths[0]
            .commands
            .iter()
            .any(|c| matches!(c, PathCommand::CubicTo { .. }));
        assert!(has_cubic, "Rounded rect should have cubic bezier corners");
    }

    #[test]
    fn break_apart_via_world_vecpath_preserves_placement() {
        use crate::object::LayerRef;
        use crate::vector::path_ops;

        // Simulate post-convert_to_path state: VectorPath with world-space coords
        let path_data = "M100 200 C100 700 200 700 200 200 Z M300 50 L400 50 L400 150 Z";
        let vp = beambench_common::path::VecPath::parse_svg_d(path_data);
        let vis = vp.visual_bounds().unwrap();
        let obj = ProjectObject::new(
            "test",
            LayerRef::new(),
            vis,
            ObjectData::VectorPath {
                path_data: path_data.to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        );

        let world = object_to_world_vecpath(&obj).unwrap();
        let parts = path_ops::break_apart(&world.to_svg_d());
        assert_eq!(parts.len(), 2);

        // First part should be near (100, 200), not offset
        let p1 = beambench_common::path::VecPath::parse_svg_d(&parts[0]);
        let b1 = p1.visual_bounds().unwrap();
        assert!(
            (b1.min.x - 100.0).abs() < 2.0,
            "min.x={} should be near 100",
            b1.min.x
        );
        assert!(
            (b1.min.y - 200.0).abs() < 2.0,
            "min.y={} should be near 200",
            b1.min.y
        );
    }

    #[test]
    fn break_apart_via_world_vecpath_with_transform() {
        use crate::object::LayerRef;
        use crate::vector::path_ops;

        let path_data = "M0 0 L100 0 L100 100 Z M200 0 L300 0 L300 100 Z";
        let vp = beambench_common::path::VecPath::parse_svg_d(path_data);
        let vis = vp.visual_bounds().unwrap();
        let mut obj = ProjectObject::new(
            "test",
            LayerRef::new(),
            vis,
            ObjectData::VectorPath {
                path_data: path_data.to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        );
        obj.transform = Transform2D::translate(50.0, 50.0);

        let world = object_to_world_vecpath(&obj).unwrap();
        let parts = path_ops::break_apart(&world.to_svg_d());
        assert_eq!(parts.len(), 2);

        let p1 = beambench_common::path::VecPath::parse_svg_d(&parts[0]);
        let b1 = p1.visual_bounds().unwrap();
        assert!(
            (b1.min.x - 50.0).abs() < 2.0,
            "min.x={} should be near 50 (0 + 50 translate)",
            b1.min.x
        );
        assert!(
            (b1.min.y - 50.0).abs() < 2.0,
            "min.y={} should be near 50 (0 + 50 translate)",
            b1.min.y
        );
    }

    #[test]
    fn object_to_world_vecpath_rotates_around_bounds_center() {
        use crate::object::LayerRef;

        let path_data = "M0 0 L20 0 L20 10 L0 10 Z";
        let vp = beambench_common::path::VecPath::parse_svg_d(path_data);
        let vis = vp.visual_bounds().unwrap();
        let mut obj = ProjectObject::new(
            "test",
            LayerRef::new(),
            vis,
            ObjectData::VectorPath {
                path_data: path_data.to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        );
        obj.transform = Transform2D::rotate(std::f64::consts::FRAC_PI_2);

        let world = object_to_world_vecpath(&obj).unwrap();
        let bounds = world.visual_bounds().unwrap();

        assert!((bounds.min.x - 5.0).abs() < 1e-6);
        assert!((bounds.min.y - (-5.0)).abs() < 1e-6);
        assert!((bounds.max.x - 15.0).abs() < 1e-6);
        assert!((bounds.max.y - 15.0).abs() < 1e-6);
    }

    #[test]
    fn object_local_point_to_world_rotates_around_bounds_center() {
        use crate::object::LayerRef;

        let path_data = "M0 0 L20 0 L20 10 L0 10 Z";
        let mut obj = ProjectObject::new(
            "test",
            LayerRef::new(),
            Bounds::new(Point2D::new(10.0, 20.0), Point2D::new(30.0, 30.0)),
            ObjectData::VectorPath {
                path_data: path_data.to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        );
        obj.transform = Transform2D::rotate(std::f64::consts::FRAC_PI_2);

        let world = object_local_point_to_world(&obj, Point2D::new(20.0, 5.0)).unwrap();

        assert!((world.x - 20.0).abs() < 1e-6);
        assert!((world.y - 35.0).abs() < 1e-6);
    }

    #[test]
    fn world_vecpath_applies_lazy_start_point_edits_on_shape() {
        use crate::object::{LayerRef, StartPointEdit};
        use beambench_common::path::PathCommand;

        // Create a rectangle Shape object (not VectorPath)
        let mut obj = ProjectObject::new(
            "rect",
            LayerRef::new(),
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );

        // Without edits: start at (0,0) corner
        let base = object_to_world_vecpath(&obj).unwrap();
        match base.subpaths[0].commands[0] {
            PathCommand::MoveTo { x, y } => {
                assert!(
                    x.abs() < 1e-6 && y.abs() < 1e-6,
                    "Base rectangle should start at (0,0), got ({x},{y})"
                );
            }
            _ => panic!("Expected MoveTo"),
        }

        // Add a start_point_edit rotating to vertex 2 (which is (10,10))
        // osci after "set v2": (0 + 4 - 2) % 4 = 2
        obj.start_point_edits.push(StartPointEdit {
            subpath_index: 0,
            original_start_current_idx: 2,
            reversed: false,
            v_display: 4,
            normalized: true,
        });

        let edited = object_to_world_vecpath(&obj).unwrap();
        match edited.subpaths[0].commands[0] {
            PathCommand::MoveTo { x, y } => {
                assert!(
                    (x - 10.0).abs() < 1e-6 && (y - 10.0).abs() < 1e-6,
                    "Edited rectangle should start at (10,10), got ({x},{y})"
                );
            }
            _ => panic!("Expected MoveTo"),
        }

        // Original ObjectData is still a Shape (not converted to VectorPath)
        assert!(
            matches!(obj.data, ObjectData::Shape { .. }),
            "Object data should still be Shape, not VectorPath"
        );
    }

    #[test]
    fn world_vecpath_no_lazy_edits_on_vector_path() {
        use crate::object::{LayerRef, StartPointEdit};
        use beambench_common::path::PathCommand;

        // VectorPath with start already rotated to (10,10)
        let mut obj = ProjectObject::new(
            "path",
            LayerRef::new(),
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::VectorPath {
                // Already rotated: starts at C(10,10)
                path_data: "M10 10 L0 10 L0 0 L10 0 L10 10 Z".to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        );
        // Metadata says osci=2 (VectorPath has edits baked in — should NOT re-apply)
        obj.start_point_edits.push(StartPointEdit {
            subpath_index: 0,
            original_start_current_idx: 2,
            reversed: false,
            v_display: 4,
            normalized: true,
        });

        let result = object_to_world_vecpath(&obj).unwrap();
        // Should still start at (10,10) — lazy edits are NOT applied to VectorPath
        match result.subpaths[0].commands[0] {
            PathCommand::MoveTo { x, y } => {
                assert!(
                    (x - 10.0).abs() < 1e-6 && (y - 10.0).abs() < 1e-6,
                    "VectorPath should not have edits re-applied, got ({x},{y})"
                );
            }
            _ => panic!("Expected MoveTo"),
        }
    }

    // --- Shape geometry tests ---

    #[test]
    fn bitten_rect_closed_subpath() {
        let path = rect_to_vecpath(100.0, 50.0, -10.0);
        assert_eq!(path.subpaths.len(), 1);
        assert!(path.subpaths[0].closed);
        // Bitten rect has 4 CubicTo arcs (one per corner)
        let cubic_count = path.subpaths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, PathCommand::CubicTo { .. }))
            .count();
        assert_eq!(cubic_count, 4, "Bitten rect should have 4 cubic arcs");
    }

    #[test]
    fn bitten_rect_preserves_dimensions() {
        let path = rect_to_vecpath(80.0, 60.0, -5.0);
        let bounds = path.visual_bounds().unwrap();
        assert!((bounds.max.x - bounds.min.x - 80.0).abs() < 0.01);
        assert!((bounds.max.y - bounds.min.y - 60.0).abs() < 0.01);
    }

    #[test]
    fn negative_corner_radius_uses_bitten_rect() {
        // rect_to_vecpath with negative radius should produce cubic arcs
        let path = rect_to_vecpath(50.0, 30.0, -5.0);
        let has_cubic = path.subpaths[0]
            .commands
            .iter()
            .any(|c| matches!(c, PathCommand::CubicTo { .. }));
        assert!(
            has_cubic,
            "Negative corner radius should produce cubic arcs"
        );
    }

    #[test]
    fn star_vertices_count() {
        // Regular star with N=5: MoveTo + 9 LineTo + Close = 11 commands
        let path = star_to_vecpath(5, 0.0, 0.5, false, None, 0.0, &[]);
        assert_eq!(path.subpaths.len(), 1);
        assert!(path.subpaths[0].closed);
        // 5 outer + 5 inner = 10 vertices: 1 MoveTo + 9 LineTo + 1 Close
        assert_eq!(path.subpaths[0].commands.len(), 11);
    }

    #[test]
    fn star_dual_radius_4n_vertices() {
        // Dual-radius star with N=5, bulge=0: 4*5 = 20 anchors
        let path = star_to_vecpath(5, 0.0, 0.4, true, Some(0.7), 0.0, &[]);
        assert_eq!(path.subpaths.len(), 1);
        assert!(path.subpaths[0].closed);
        assert_eq!(
            path.subpaths[0].commands.len(),
            21,
            "Dual-radius star with N=5 should have 4*5+1=21 commands (MoveTo + 19 LineTo + Close)"
        );
    }

    #[test]
    fn star_dual_radius_tip_and_valley_radii() {
        // Verify the dual star uses primary tips at outer radius, valleys at ratio,
        // and secondary tips at ratio2.
        let path = star_to_vecpath(4, 0.0, 0.3, true, Some(0.6), 0.0, &[]);
        let cmds = &path.subpaths[0].commands;
        let outer_radius = 50.0;
        let valley_radius = outer_radius * 0.3_f64;
        let secondary_radius = outer_radius * 0.6_f64;

        let mut vertex_radii: Vec<f64> = Vec::new();
        for (i, cmd) in cmds.iter().enumerate() {
            if i == cmds.len() - 1 {
                continue;
            }
            match cmd {
                PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => {
                    let dx = x - outer_radius;
                    let dy = y - outer_radius;
                    vertex_radii.push((dx * dx + dy * dy).sqrt());
                }
                _ => {}
            }
        }
        assert_eq!(vertex_radii.len(), 16);
        for (j, r) in vertex_radii.iter().enumerate() {
            let expected = match j % 4 {
                0 => outer_radius,
                1 | 3 => valley_radius,
                _ => secondary_radius,
            };
            assert!(
                (r - expected).abs() < 0.01,
                "Vertex {j}: expected radius {expected:.2}, got {r:.2}"
            );
        }
    }

    #[test]
    fn star_regular_ratio_tracks_valley_radius() {
        let path = star_to_vecpath(3, 0.0, 0.15, false, None, 0.0, &[]);
        let cmds = &path.subpaths[0].commands;
        let outer_radius = 50.0;
        let expected_valley_radius = outer_radius * 0.15_f64;

        let mut radii = Vec::new();
        for cmd in cmds.iter().take(cmds.len() - 1) {
            match cmd {
                PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => {
                    let dx = x - outer_radius;
                    let dy = y - outer_radius;
                    radii.push((dx * dx + dy * dy).sqrt());
                }
                _ => {}
            }
        }
        assert_eq!(radii.len(), 6);
        for (idx, radius) in radii.iter().enumerate() {
            let expected = if idx.is_multiple_of(2) {
                outer_radius
            } else {
                expected_valley_radius
            };
            assert!(
                (radius - expected).abs() < 0.01,
                "Regular star vertex {idx}: expected radius {expected:.2}, got {radius:.2}"
            );
        }
    }

    #[test]
    fn star_bulge_produces_curves() {
        // Non-zero bulge should emit CubicTo commands instead of LineTo
        let path = star_to_vecpath(5, 1.0, 0.5, false, None, 0.0, &[]);
        let has_cubic = path.subpaths[0]
            .commands
            .iter()
            .any(|c| matches!(c, PathCommand::CubicTo { .. }));
        assert!(has_cubic, "Non-zero bulge should produce curved edges");
        // Should have no LineTo (all edges curved except MoveTo and Close)
        let line_count = path.subpaths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, PathCommand::LineTo { .. }))
            .count();
        assert_eq!(
            line_count, 0,
            "All edges should be curves when bulge is non-zero"
        );
    }

    #[test]
    fn star_bulge_preserves_anchor_positions() {
        let straight = star_to_vecpath(3, 0.0, 0.15, false, None, 0.0, &[]);
        let curved = star_to_vecpath(3, 1.0, 0.15, false, None, 0.0, &[]);

        let straight_points: Vec<(f64, f64)> = straight.subpaths[0]
            .commands
            .iter()
            .filter_map(|cmd| match cmd {
                PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => Some((*x, *y)),
                _ => None,
            })
            .collect();
        let curved_points: Vec<(f64, f64)> = curved.subpaths[0]
            .commands
            .iter()
            .filter_map(|cmd| match cmd {
                PathCommand::MoveTo { x, y } => Some((*x, *y)),
                PathCommand::CubicTo { x, y, .. } => Some((*x, *y)),
                _ => None,
            })
            .collect();
        let curved_points = if curved_points.len() == straight_points.len() + 1
            && curved_points.first() == curved_points.last()
        {
            &curved_points[..curved_points.len() - 1]
        } else {
            &curved_points[..]
        };

        assert_eq!(straight_points.len(), curved_points.len());
        for (idx, ((sx, sy), (cx, cy))) in
            straight_points.iter().zip(curved_points.iter()).enumerate()
        {
            assert!((sx - cx).abs() < 1e-6, "Anchor {idx} x moved: {sx} vs {cx}");
            assert!((sy - cy).abs() < 1e-6, "Anchor {idx} y moved: {sy} vs {cy}");
        }
    }

    #[test]
    fn star_corner_radius_produces_curves() {
        let path = star_to_vecpath(5, 0.0, 0.5, false, None, 8.0, &[]);
        let cubic_count = path.subpaths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, PathCommand::CubicTo { .. }))
            .count();
        assert_eq!(
            cubic_count, 10,
            "Uniform rounded 5-point star should round all 10 anchors"
        );
    }

    #[test]
    fn star_per_corner_radius_rounds_single_anchor() {
        let mut radii = vec![0.0; 10];
        radii[0] = 8.0;
        let path = star_to_vecpath(5, 0.0, 0.5, false, None, 0.0, &radii);
        let cubic_count = path.subpaths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, PathCommand::CubicTo { .. }))
            .count();
        assert_eq!(
            cubic_count, 1,
            "Single-corner rounded star should emit one cubic corner"
        );
    }

    #[test]
    fn star_new_object_deserializes_with_defaults() {
        // Star JSON with only required field → defaults apply
        let json = r#"{"type":"star","points":5}"#;
        let data: ObjectData = serde_json::from_str(json).unwrap();
        if let ObjectData::Star {
            points,
            bulge,
            ratio,
            dual_radius,
            ratio2,
            corner_radius,
            corner_radii,
        } = data
        {
            assert_eq!(points, 5);
            assert!((bulge - 0.0).abs() < 1e-9);
            assert!((ratio - 0.5).abs() < 1e-9, "Default ratio should be 0.5");
            assert!(!dual_radius);
            assert!(ratio2.is_none());
            assert!((corner_radius - 0.0).abs() < 1e-9);
            assert!(corner_radii.is_empty());
        } else {
            panic!("Expected ObjectData::Star");
        }
    }
}

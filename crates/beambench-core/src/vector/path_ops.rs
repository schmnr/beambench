use beambench_common::Point2D;
use beambench_common::path::{PathCommand, SubPath, VecPath};
use geo::algorithm::simplify::Simplify;
use geo::{Coord, LineString};
use serde::{Deserialize, Serialize};

use crate::object::{ObjectData, ObjectId, ProjectObject};
use crate::vector::flatten::{DEFAULT_TOLERANCE_MM, flatten_vecpath};

/// Force-close an open SVG path by appending Z if not already closed.
pub fn close_path(path_data: &str) -> String {
    let trimmed = path_data.trim();
    if trimmed.is_empty() {
        return trimmed.to_string();
    }

    // Check if already closed
    if trimmed.ends_with('Z') || trimmed.ends_with('z') {
        return trimmed.to_string();
    }

    format!("{} Z", trimmed)
}

/// Close paths whose endpoints are within tolerance.
pub fn close_paths_with_tolerance(paths: &[String], tolerance_mm: f64) -> Vec<String> {
    paths
        .iter()
        .map(|path_str| {
            let path = VecPath::parse_svg_d(path_str);
            let mut result_subpaths = Vec::new();

            for sp in &path.subpaths {
                if sp.closed || sp.commands.len() < 2 {
                    result_subpaths.push(sp.clone());
                    continue;
                }

                // Get first and last point
                let first = match sp.commands.first() {
                    Some(PathCommand::MoveTo { x, y }) => Point2D { x: *x, y: *y },
                    _ => {
                        result_subpaths.push(sp.clone());
                        continue;
                    }
                };

                let last = match sp.commands.last() {
                    Some(PathCommand::LineTo { x, y })
                    | Some(PathCommand::QuadTo { x, y, .. })
                    | Some(PathCommand::CubicTo { x, y, .. }) => Point2D { x: *x, y: *y },
                    _ => {
                        result_subpaths.push(sp.clone());
                        continue;
                    }
                };

                let distance = ((last.x - first.x).powi(2) + (last.y - first.y).powi(2)).sqrt();

                if distance <= tolerance_mm {
                    let mut new_commands = sp.commands.clone();
                    new_commands.push(PathCommand::Close);
                    result_subpaths.push(SubPath {
                        commands: new_commands,
                        closed: true,
                    });
                } else {
                    result_subpaths.push(sp.clone());
                }
            }

            VecPath {
                subpaths: result_subpaths,
            }
            .to_svg_d()
        })
        .collect()
}

/// Merge paths whose endpoints are within tolerance.
pub fn auto_join_paths(paths: &[VecPath], tolerance: f64) -> Vec<VecPath> {
    if paths.is_empty() {
        return vec![];
    }

    let mut remaining: Vec<VecPath> = paths.to_vec();
    let mut joined = Vec::new();

    while !remaining.is_empty() {
        let mut current = remaining.remove(0);
        let mut changed = true;

        while changed {
            changed = false;

            for i in (0..remaining.len()).rev() {
                if can_join(&current, &remaining[i], tolerance) {
                    current = join_two_paths(&current, &remaining.remove(i));
                    changed = true;
                    break;
                }
            }
        }

        joined.push(current);
    }

    joined
}

fn can_join(a: &VecPath, b: &VecPath, tolerance: f64) -> bool {
    if a.subpaths.is_empty() || b.subpaths.is_empty() {
        return false;
    }

    let a_last = get_last_point(&a.subpaths[a.subpaths.len() - 1]);
    let b_first = get_first_point(&b.subpaths[0]);

    if let (Some(a_pt), Some(b_pt)) = (a_last, b_first) {
        let dist = ((a_pt.x - b_pt.x).powi(2) + (a_pt.y - b_pt.y).powi(2)).sqrt();
        dist <= tolerance
    } else {
        false
    }
}

fn join_two_paths(a: &VecPath, b: &VecPath) -> VecPath {
    let mut subpaths = a.subpaths.clone();
    subpaths.extend(b.subpaths.clone());
    VecPath { subpaths }
}

fn get_first_point(sp: &SubPath) -> Option<Point2D> {
    sp.commands.first().and_then(|cmd| match cmd {
        PathCommand::MoveTo { x, y } => Some(Point2D { x: *x, y: *y }),
        _ => None,
    })
}

fn get_last_point(sp: &SubPath) -> Option<Point2D> {
    for cmd in sp.commands.iter().rev() {
        match cmd {
            PathCommand::LineTo { x, y }
            | PathCommand::QuadTo { x, y, .. }
            | PathCommand::CubicTo { x, y, .. } => return Some(Point2D { x: *x, y: *y }),
            PathCommand::Close => continue,
            PathCommand::MoveTo { .. } => return None,
        }
    }
    None
}

/// Remove redundant points and simplify using geo::Simplify.
pub fn optimize_path(path: &VecPath, tolerance: f64) -> VecPath {
    let mut result_subpaths = Vec::new();

    for polyline in flatten_vecpath(path, DEFAULT_TOLERANCE_MM) {
        if polyline.points.len() < 2 {
            continue;
        }

        let coords: Vec<Coord<f64>> = polyline
            .points
            .iter()
            .map(|p| Coord { x: p.x, y: p.y })
            .collect();

        let line = LineString::new(coords);
        let simplified = line.simplify(&tolerance);

        let simplified_points: Vec<&Coord<f64>> = simplified.coords().collect();

        if simplified_points.len() >= 2 {
            let mut commands = Vec::new();
            commands.push(PathCommand::MoveTo {
                x: simplified_points[0].x,
                y: simplified_points[0].y,
            });

            for pt in &simplified_points[1..] {
                commands.push(PathCommand::LineTo { x: pt.x, y: pt.y });
            }

            if polyline.closed {
                commands.push(PathCommand::Close);
            }

            result_subpaths.push(SubPath {
                commands,
                closed: polyline.closed,
            });
        }
    }

    VecPath {
        subpaths: result_subpaths,
    }
}

/// Find duplicate objects by comparing bounds and path data.
pub fn find_duplicates(objects: &[ProjectObject]) -> Vec<ObjectId> {
    let mut duplicates = Vec::new();

    for i in 0..objects.len() {
        for j in (i + 1)..objects.len() {
            if objects_are_duplicate(&objects[i], &objects[j])
                && !duplicates.contains(&objects[j].id)
            {
                duplicates.push(objects[j].id);
            }
        }
    }

    duplicates
}

fn objects_are_duplicate(a: &ProjectObject, b: &ProjectObject) -> bool {
    // Compare bounds
    if (a.bounds.min.x - b.bounds.min.x).abs() > 0.01
        || (a.bounds.min.y - b.bounds.min.y).abs() > 0.01
        || (a.bounds.max.x - b.bounds.max.x).abs() > 0.01
        || (a.bounds.max.y - b.bounds.max.y).abs() > 0.01
    {
        return false;
    }

    // Compare object data
    a.data == b.data
}

/// Break a path into its constituent pieces.
///
/// - Multiple sub-paths → split into individual sub-paths.
/// - Single sub-path with multiple segments → break at each segment
///   boundary into individual open curves.
///   For closed sub-paths, a closing edge (LineTo back to start) is emitted
///   when the last drawing command doesn't already end at the start point.
/// - Single sub-path with one segment → returns as-is (nothing to break).
pub fn break_apart(path_data: &str) -> Vec<String> {
    let path = VecPath::parse_svg_d(path_data);

    // Multiple sub-paths: split each into its own VecPath
    if path.subpaths.len() > 1 {
        return path
            .subpaths
            .into_iter()
            .map(|sp| VecPath { subpaths: vec![sp] }.to_svg_d())
            .collect();
    }

    // Single sub-path: break into individual segments
    let Some(sp) = path.subpaths.into_iter().next() else {
        return vec![];
    };

    // Walk the commands, tracking current position, and emit each segment
    // as its own M...segment sub-path
    let mut result = Vec::new();
    let mut curr_x = 0.0;
    let mut curr_y = 0.0;
    let mut start_x = 0.0;
    let mut start_y = 0.0;

    for cmd in &sp.commands {
        match *cmd {
            PathCommand::MoveTo { x, y } => {
                curr_x = x;
                curr_y = y;
                start_x = x;
                start_y = y;
            }
            PathCommand::LineTo { x, y } => {
                let mut seg_sp = SubPath::new();
                seg_sp.commands.push(PathCommand::MoveTo {
                    x: curr_x,
                    y: curr_y,
                });
                seg_sp.commands.push(PathCommand::LineTo { x, y });
                result.push(
                    VecPath {
                        subpaths: vec![seg_sp],
                    }
                    .to_svg_d(),
                );
                curr_x = x;
                curr_y = y;
            }
            PathCommand::QuadTo { cx, cy, x, y } => {
                let mut seg_sp = SubPath::new();
                seg_sp.commands.push(PathCommand::MoveTo {
                    x: curr_x,
                    y: curr_y,
                });
                seg_sp.commands.push(PathCommand::QuadTo { cx, cy, x, y });
                result.push(
                    VecPath {
                        subpaths: vec![seg_sp],
                    }
                    .to_svg_d(),
                );
                curr_x = x;
                curr_y = y;
            }
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                let mut seg_sp = SubPath::new();
                seg_sp.commands.push(PathCommand::MoveTo {
                    x: curr_x,
                    y: curr_y,
                });
                seg_sp.commands.push(PathCommand::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                });
                result.push(
                    VecPath {
                        subpaths: vec![seg_sp],
                    }
                    .to_svg_d(),
                );
                curr_x = x;
                curr_y = y;
            }
            PathCommand::Close => {
                // Emit closing edge if current point differs from start
                let dx = curr_x - start_x;
                let dy = curr_y - start_y;
                if dx * dx + dy * dy > 1e-12 {
                    let mut seg_sp = SubPath::new();
                    seg_sp.commands.push(PathCommand::MoveTo {
                        x: curr_x,
                        y: curr_y,
                    });
                    seg_sp.commands.push(PathCommand::LineTo {
                        x: start_x,
                        y: start_y,
                    });
                    result.push(
                        VecPath {
                            subpaths: vec![seg_sp],
                        }
                        .to_svg_d(),
                    );
                }
                curr_x = start_x;
                curr_y = start_y;
            }
        }
    }

    // If only 0 or 1 segments resulted, nothing to break — return original
    if result.len() <= 1 {
        return vec![VecPath { subpaths: vec![sp] }.to_svg_d()];
    }

    result
}

// ────────────────────────────────────────────────────────���───
// Path vertex info for canvas overlay
// ────────────────────────────────────────────────────────────

/// A vertex anchor on a path, used by the start-point pick overlay.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathVertex {
    pub subpath_index: usize,
    pub vertex_index: usize,
    pub x: f64,
    pub y: f64,
    pub is_start: bool,
    pub subpath_closed: bool,
}

/// Extract display vertices from a VecPath for canvas overlay rendering.
///
/// For closed subpaths, if the last draw command is a LineTo whose endpoint
/// matches the MoveTo endpoint (within 1e-9), it is suppressed as a
/// normalized closing vertex.
pub fn get_path_vertices(path: &VecPath) -> Vec<PathVertex> {
    let mut result = Vec::new();
    for (sp_idx, sp) in path.subpaths.iter().enumerate() {
        let mut vertices: Vec<(usize, f64, f64)> = Vec::new();
        for (i, cmd) in sp.commands.iter().enumerate() {
            match *cmd {
                PathCommand::MoveTo { x, y } => vertices.push((i, x, y)),
                PathCommand::LineTo { x, y }
                | PathCommand::QuadTo { x, y, .. }
                | PathCommand::CubicTo { x, y, .. } => vertices.push((i, x, y)),
                PathCommand::Close => {}
            }
        }

        // Suppress normalized closing vertex for closed subpaths
        if sp.closed && vertices.len() >= 2 {
            let first = &vertices[0];
            let last = &vertices[vertices.len() - 1];
            let dx = last.1 - first.1;
            let dy = last.2 - first.2;
            if dx * dx + dy * dy < 1e-18 {
                // Check that the suppressed command is a LineTo (normalized closing edge)
                if let Some(PathCommand::LineTo { .. }) = sp.commands.get(last.0) {
                    vertices.pop();
                }
            }
        }

        for (v_idx, &(_, x, y)) in vertices.iter().enumerate() {
            result.push(PathVertex {
                subpath_index: sp_idx,
                vertex_index: v_idx,
                x,
                y,
                is_start: v_idx == 0,
                subpath_closed: sp.closed,
            });
        }
    }
    result
}

// ────────────────────────────────────────────────────────────
// Normalization / denormalization for start-point editing
// ────────────────────────────────────────────────────────────

/// Normalize a closed subpath so that Close is degenerate (last draw endpoint == MoveTo endpoint).
/// Returns `(normalized_subpath, was_modified)`.
pub fn normalize_closed_subpath(sp: &SubPath) -> (SubPath, bool) {
    if !sp.closed {
        return (sp.clone(), false);
    }
    let first_pt = match sp.commands.first() {
        Some(PathCommand::MoveTo { x, y }) => Point2D::new(*x, *y),
        _ => return (sp.clone(), false),
    };
    let last_pt = match command_endpoint(
        sp.commands
            .iter()
            .rev()
            .find(|c| !matches!(c, PathCommand::Close)),
    ) {
        Some(p) => p,
        None => return (sp.clone(), false),
    };
    if first_pt.distance_to(&last_pt) < 1e-9 {
        return (sp.clone(), false);
    }
    // Insert LineTo before Close
    let mut cmds = Vec::with_capacity(sp.commands.len() + 1);
    for cmd in &sp.commands {
        if matches!(cmd, PathCommand::Close) {
            cmds.push(PathCommand::LineTo {
                x: first_pt.x,
                y: first_pt.y,
            });
        }
        cmds.push(*cmd);
    }
    (
        SubPath {
            commands: cmds,
            closed: true,
        },
        true,
    )
}

/// Remove the closing LineTo added by normalization (if the last draw command
/// before Close is a LineTo matching MoveTo endpoint within 1e-9).
pub fn denormalize_closed_subpath(sp: &SubPath) -> SubPath {
    if !sp.closed || sp.commands.len() < 3 {
        return sp.clone();
    }
    let first_pt = match sp.commands.first() {
        Some(PathCommand::MoveTo { x, y }) => Point2D::new(*x, *y),
        _ => return sp.clone(),
    };
    // Find last non-Close command
    let last_draw_idx = sp
        .commands
        .iter()
        .rposition(|c| !matches!(c, PathCommand::Close));
    if let Some(idx) = last_draw_idx {
        if let PathCommand::LineTo { x, y } = sp.commands[idx] {
            let dx = x - first_pt.x;
            let dy = y - first_pt.y;
            if dx * dx + dy * dy < 1e-18 {
                let mut cmds: Vec<PathCommand> = sp
                    .commands
                    .iter()
                    .enumerate()
                    .filter(|&(i, _)| i != idx)
                    .map(|(_, c)| *c)
                    .collect();
                if !cmds.iter().any(|c| matches!(c, PathCommand::Close)) {
                    cmds.push(PathCommand::Close);
                }
                return SubPath {
                    commands: cmds,
                    closed: true,
                };
            }
        }
    }
    sp.clone()
}

/// Denormalize all start-point edits on a project object, then clear the edits.
/// Call this before any mutation that reads path_data for a non-start-point operation.
pub fn ensure_denormalized(obj: &mut ProjectObject) {
    if obj.start_point_edits.is_empty() {
        return;
    }
    if let ObjectData::VectorPath {
        ref mut path_data, ..
    } = obj.data
    {
        let mut vp = VecPath::parse_svg_d(path_data);
        for entry in &obj.start_point_edits {
            if entry.normalized && entry.subpath_index < vp.subpaths.len() {
                vp.subpaths[entry.subpath_index] =
                    denormalize_closed_subpath(&vp.subpaths[entry.subpath_index]);
            }
        }
        *path_data = vp.to_svg_d();
    }
    obj.start_point_edits.clear();
}

/// Apply start-point edits in the forward direction to a VecPath.
///
/// This is used for non-VectorPath objects (Shape, Text, Polygon) where
/// start_point_edits are stored as metadata but NOT baked into path data.
/// Each call to `object_to_world_vecpath` generates the original path from
/// the primitive, and this function applies the stored normalize → rotate →
/// reverse operations so downstream consumers (planner, export, canvas) see
/// the edited start point.
pub fn apply_start_point_edits_forward(
    vp: &VecPath,
    edits: &[crate::object::StartPointEdit],
) -> VecPath {
    let mut result = vp.clone();
    for edit in edits {
        if edit.subpath_index >= result.subpaths.len() {
            continue;
        }
        // Step 1: Normalize the subpath if the edit was created with normalization
        if edit.normalized {
            let (norm_sp, _) = normalize_closed_subpath(&result.subpaths[edit.subpath_index]);
            result.subpaths[edit.subpath_index] = norm_sp;
        }
        // Step 2: Compute forward rotation from original_start_current_idx
        // osci tracks where the original vertex 0 currently sits.
        // Forward rotation = (v_display - osci) % v_display moves it there.
        let v = edit.v_display;
        if v > 0 {
            let fwd = (v - edit.original_start_current_idx) % v;
            if fwd > 0 {
                result = rotate_subpath_start(&result, edit.subpath_index, fwd);
            }
        }
        // Step 3: Apply reversal if the edit indicates direction was reversed
        if edit.reversed {
            result = reverse_subpath_at(&result, edit.subpath_index);
        }
    }
    result
}

// ────────────────────────────────────────────────────────────
// Subpath rotation and reversal
// ────────────────────────────────────────────────────────────

/// Reverse only the subpath at `subpath_idx`, passing others through unchanged.
/// Delegates to `trim::reverse_subpath` which preserves curves correctly.
pub fn reverse_subpath_at(path: &VecPath, subpath_idx: usize) -> VecPath {
    let mut subpaths = path.subpaths.clone();
    if subpath_idx < subpaths.len() {
        subpaths[subpath_idx] = crate::vector::trim::reverse_subpath(&subpaths[subpath_idx]);
    }
    VecPath { subpaths }
}

/// Rotate a normalized closed subpath so the display vertex at `vertex_idx`
/// becomes the start. Open subpaths and non-matching indices pass through unchanged.
///
/// Precondition: The target subpath MUST be normalized (Close is degenerate).
pub fn rotate_subpath_start(path: &VecPath, subpath_idx: usize, vertex_idx: usize) -> VecPath {
    if vertex_idx == 0 {
        return path.clone();
    }
    let mut subpaths = path.subpaths.clone();
    let sp = match subpaths.get(subpath_idx) {
        Some(sp) if sp.closed => sp,
        _ => return path.clone(),
    };

    // Collect segments: (start_point, draw_command)
    let mut segments: Vec<(Point2D, PathCommand)> = Vec::new();
    let mut current = Point2D::zero();
    for cmd in &sp.commands {
        match *cmd {
            PathCommand::MoveTo { x, y } => {
                current = Point2D::new(x, y);
            }
            PathCommand::Close => {}
            _ => {
                segments.push((current, *cmd));
                current = command_endpoint(Some(cmd)).unwrap_or(current);
            }
        }
    }

    let v_display = segments.len();
    if vertex_idx >= v_display || v_display == 0 {
        return path.clone();
    }

    // Rotate segments
    segments.rotate_left(vertex_idx);

    // Emit new commands
    let mut cmds = Vec::with_capacity(segments.len() + 2);
    cmds.push(PathCommand::MoveTo {
        x: segments[0].0.x,
        y: segments[0].0.y,
    });
    for &(_, cmd) in &segments {
        cmds.push(cmd);
    }
    cmds.push(PathCommand::Close);

    subpaths[subpath_idx] = SubPath {
        commands: cmds,
        closed: true,
    };
    VecPath { subpaths }
}

/// Helper: extract endpoint from a command.
fn command_endpoint(cmd: Option<&PathCommand>) -> Option<Point2D> {
    match cmd? {
        PathCommand::MoveTo { x, y }
        | PathCommand::LineTo { x, y }
        | PathCommand::QuadTo { x, y, .. }
        | PathCommand::CubicTo { x, y, .. } => Some(Point2D::new(*x, *y)),
        PathCommand::Close => None,
    }
}

// ────────────────────────────────────────────────────────────
// Arc-based fillet (apply_radius)
// ────────────────────────────────────────────────────────────

/// A corner on a VecPath that qualifies for filleting (LineTo→LineTo) or is already filleted
/// (LineTo→CubicTo→LineTo where the CubicTo is a fillet arc).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilletCandidate {
    pub subpath_index: usize,
    /// Index into the fillet_info array (= cmd index in all_cmds for the incoming edge).
    pub vertex_index: usize,
    pub x: f64,
    pub y: f64,
    /// True if this corner already has a fillet arc (CubicTo between two LineTos).
    pub already_filleted: bool,
}

/// Detect which corners in a VecPath qualify for filleting (LineTo→LineTo)
/// or are already filleted (LineTo→CubicTo→LineTo).
pub fn get_fillet_candidates(path: &VecPath) -> Vec<FilletCandidate> {
    let mut result = Vec::new();
    for (sp_idx, sp) in path.subpaths.iter().enumerate() {
        let mut draw_cmds: Vec<(Point2D, PathCommand)> = Vec::new();
        let mut move_pt = Point2D::zero();
        let mut current = Point2D::zero();

        for cmd in &sp.commands {
            match *cmd {
                PathCommand::MoveTo { x, y } => {
                    move_pt = Point2D::new(x, y);
                    current = move_pt;
                }
                PathCommand::Close => {}
                _ => {
                    let ep = command_endpoint(Some(cmd)).unwrap_or(current);
                    draw_cmds.push((current, *cmd));
                    current = ep;
                }
            }
        }

        if draw_cmds.len() < 2 {
            continue;
        }

        let n = draw_cmds.len();
        let is_closed = sp.closed;

        let closing_edge: Option<(Point2D, PathCommand)> = if is_closed {
            let last_ep = command_endpoint(Some(&draw_cmds[n - 1].1)).unwrap_or(current);
            if last_ep.distance_to(&move_pt) > 1e-9 {
                Some((
                    last_ep,
                    PathCommand::LineTo {
                        x: move_pt.x,
                        y: move_pt.y,
                    },
                ))
            } else {
                None
            }
        } else {
            None
        };

        let full_len = n + if closing_edge.is_some() { 1 } else { 0 };
        let mut all_cmds: Vec<(Point2D, PathCommand)> = draw_cmds;
        if let Some(ce) = closing_edge {
            all_cmds.push(ce);
        }

        for i in 0..full_len {
            // Skip first vertex for open paths (no incoming edge)
            if i == 0 && !is_closed {
                continue;
            }
            let next_idx = if is_closed {
                (i + 1) % full_len
            } else {
                if i + 1 >= full_len {
                    continue;
                }
                i + 1
            };

            let curr_is_line = matches!(all_cmds[i].1, PathCommand::LineTo { .. });
            let next_is_line = matches!(all_cmds[next_idx].1, PathCommand::LineTo { .. });

            if curr_is_line && next_is_line {
                // Fresh unfilleted corner
                let corner_pt = command_endpoint(Some(&all_cmds[i].1)).unwrap_or(all_cmds[i].0);
                result.push(FilletCandidate {
                    subpath_index: sp_idx,
                    vertex_index: i,
                    x: corner_pt.x,
                    y: corner_pt.y,
                    already_filleted: false,
                });
                continue;
            }

            // Check for already-filleted corner: LineTo → CubicTo+ → LineTo
            // Detect contiguous runs of CubicTos that form a fillet arc.
            let curr_is_cubic = matches!(all_cmds[i].1, PathCommand::CubicTo { .. });
            if !curr_is_cubic {
                continue;
            }

            // Incoming edge must be LineTo
            let prev_idx = if is_closed {
                (i + full_len - 1) % full_len
            } else {
                if i == 0 {
                    continue;
                }
                i - 1
            };
            let prev_is_line = matches!(all_cmds[prev_idx].1, PathCommand::LineTo { .. });
            if !prev_is_line {
                continue;
            }

            // Skip if the previous command is also a CubicTo preceded by a LineTo —
            // that means we already processed this run from its first cubic.
            if i > 0 || is_closed {
                let before_prev = if is_closed {
                    (prev_idx + full_len - 1) % full_len
                } else {
                    if prev_idx > 0 { prev_idx - 1 } else { full_len }
                };
                if before_prev < full_len
                    && matches!(all_cmds[prev_idx].1, PathCommand::CubicTo { .. })
                    && matches!(all_cmds[before_prev].1, PathCommand::LineTo { .. })
                {
                    // This CubicTo is not the first in its run — skip
                    continue;
                }
            }

            // Scan forward through contiguous CubicTos to find the run end
            let mut run_end = i; // inclusive
            loop {
                let after = if is_closed {
                    (run_end + 1) % full_len
                } else {
                    if run_end + 1 >= full_len {
                        break;
                    }
                    run_end + 1
                };
                if matches!(all_cmds[after].1, PathCommand::CubicTo { .. }) {
                    run_end = after;
                    // Safety: don't wrap around past the start in closed paths
                    if run_end == prev_idx {
                        break;
                    }
                } else {
                    break;
                }
            }

            // The command after the run must be LineTo
            let after_run = if is_closed {
                (run_end + 1) % full_len
            } else {
                if run_end + 1 >= full_len {
                    continue;
                }
                run_end + 1
            };
            if !matches!(all_cmds[after_run].1, PathCommand::LineTo { .. }) {
                continue;
            }

            // Collect cubics in the run
            let tangent_in = all_cmds[i].0;
            let line_in_start = all_cmds[prev_idx].0;
            let tangent_out = command_endpoint(Some(&all_cmds[run_end].1)).unwrap_or(tangent_in);
            let line_out_end =
                command_endpoint(Some(&all_cmds[after_run].1)).unwrap_or(tangent_out);

            let mut cubics_in_run = Vec::new();
            let mut idx = i;
            loop {
                if let PathCommand::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                } = all_cmds[idx].1
                {
                    cubics_in_run.push((
                        Point2D::new(c1x, c1y),
                        Point2D::new(c2x, c2y),
                        Point2D::new(x, y),
                    ));
                }
                if idx == run_end {
                    break;
                }
                idx = if is_closed {
                    (idx + 1) % full_len
                } else {
                    idx + 1
                };
            }

            let is_fillet = if cubics_in_run.len() == 1 {
                let (c1, c2, _) = cubics_in_run[0];
                is_fillet_cubic(line_in_start, tangent_in, c1, c2, tangent_out, line_out_end)
            } else {
                is_multi_fillet_cubic(
                    line_in_start,
                    tangent_in,
                    &cubics_in_run,
                    tangent_out,
                    line_out_end,
                )
            };

            if !is_fillet {
                continue;
            }

            if let Some(corner) =
                line_line_intersection(line_in_start, tangent_in, tangent_out, line_out_end)
            {
                result.push(FilletCandidate {
                    subpath_index: sp_idx,
                    vertex_index: i,
                    x: corner.x,
                    y: corner.y,
                    already_filleted: true,
                });
            }
        }
    }
    result
}

/// Check whether a CubicTo between two LineTos is geometrically consistent with a fillet arc.
///
/// Recognizes two fillet types:
/// 1. **Tangent fillet** (positive radius): control points collinear with edge directions,
///    arc tangent to both edges at the junction points.
/// 2. **Dog-bone fillet** (negative radius): arc centered at the virtual corner vertex,
///    endpoints equidistant from the corner, midpoint on the same circle.
fn is_fillet_cubic(
    line_in_start: Point2D,
    tangent_in: Point2D,  // CubicTo start = end of preceding LineTo
    c1: Point2D,          // first control point
    c2: Point2D,          // second control point
    tangent_out: Point2D, // CubicTo end = start of following LineTo
    line_out_end: Point2D,
) -> bool {
    // --- Check A: tangent-continuous fillet (positive radius) ---
    if is_tangent_fillet(line_in_start, tangent_in, c1, c2, tangent_out, line_out_end) {
        return true;
    }

    // --- Check B: dog-bone fillet (negative radius) ---
    is_dogbone_fillet(
        line_in_start,
        tangent_in,
        tangent_out,
        line_out_end,
        &[(c1, c2, tangent_out)],
    )
}

/// Dog-bone fillet check: arc centered at the virtual corner. First/last tangent points
/// and the midpoint of each cubic should all lie roughly on the same circle centered at
/// the line-line intersection (the virtual corner).
fn is_dogbone_fillet(
    line_in_start: Point2D,
    tangent_in: Point2D,
    tangent_out: Point2D,
    line_out_end: Point2D,
    cubics: &[(Point2D, Point2D, Point2D)],
) -> bool {
    let Some(corner) = line_line_intersection(line_in_start, tangent_in, tangent_out, line_out_end)
    else {
        return false;
    };

    let dist_in = corner.distance_to(&tangent_in);
    let dist_out = corner.distance_to(&tangent_out);
    let max_dist = dist_in.max(dist_out);
    if max_dist < 1e-9 {
        return false;
    }

    // Endpoints equidistant from virtual corner (5% tolerance)
    if (dist_in - dist_out).abs() / max_dist > 0.05 {
        return false;
    }

    // Check midpoint of each cubic lies on the same circle
    let avg_dist = (dist_in + dist_out) / 2.0;
    let mut prev_ep = tangent_in;
    for &(c1, c2, ep) in cubics {
        let mid = Point2D::new(
            0.125 * prev_ep.x + 0.375 * c1.x + 0.375 * c2.x + 0.125 * ep.x,
            0.125 * prev_ep.y + 0.375 * c1.y + 0.375 * c2.y + 0.125 * ep.y,
        );
        let dist_mid = corner.distance_to(&mid);
        if (dist_mid - avg_dist).abs() / avg_dist > 0.1 {
            return false;
        }
        prev_ep = ep;
    }

    true
}

/// Check whether a contiguous run of CubicTos between two LineTos is a multi-segment fillet.
///
/// Returns true for fillet arcs split across multiple cubic segments (e.g. 60° corners
/// produce 2 cubics). Validates first cubic's c1 tangent to incoming line and last cubic's
/// c2 tangent to outgoing line.
fn is_multi_fillet_cubic(
    line_in_start: Point2D,
    tangent_in: Point2D,
    cubics: &[(Point2D, Point2D, Point2D)], // (c1, c2, endpoint) per segment
    tangent_out: Point2D,
    line_out_end: Point2D,
) -> bool {
    if cubics.is_empty() {
        return false;
    }

    // --- Check A: tangent-continuous fillet ---
    let (first_c1, _, _) = cubics[0];
    let (_, last_c2, _) = *cubics.last().unwrap();
    if is_tangent_fillet(
        line_in_start,
        tangent_in,
        first_c1,
        last_c2,
        tangent_out,
        line_out_end,
    ) {
        return true;
    }

    // --- Check B: dog-bone fillet ---
    is_dogbone_fillet(line_in_start, tangent_in, tangent_out, line_out_end, cubics)
}

/// Tangent-continuous fillet check: control points collinear with adjacent edge directions.
fn is_tangent_fillet(
    line_in_start: Point2D,
    tangent_in: Point2D,
    c1: Point2D,
    c2: Point2D,
    tangent_out: Point2D,
    line_out_end: Point2D,
) -> bool {
    let in_dx = tangent_in.x - line_in_start.x;
    let in_dy = tangent_in.y - line_in_start.y;
    let in_len = (in_dx * in_dx + in_dy * in_dy).sqrt();
    if in_len < 1e-9 {
        return false;
    }

    let out_dx = line_out_end.x - tangent_out.x;
    let out_dy = line_out_end.y - tangent_out.y;
    let out_len = (out_dx * out_dx + out_dy * out_dy).sqrt();
    if out_len < 1e-9 {
        return false;
    }

    // c1 collinear with incoming line direction (~3° tolerance)
    let c1_dx = c1.x - tangent_in.x;
    let c1_dy = c1.y - tangent_in.y;
    let c1_len = (c1_dx * c1_dx + c1_dy * c1_dy).sqrt();
    if c1_len < 1e-9 {
        return false;
    }
    let cross1 = c1_dx * in_dy - c1_dy * in_dx;
    if cross1.abs() / (c1_len * in_len) > 0.05 {
        return false;
    }

    // c2 collinear with outgoing line direction
    let c2_dx = c2.x - tangent_out.x;
    let c2_dy = c2.y - tangent_out.y;
    let c2_len = (c2_dx * c2_dx + c2_dy * c2_dy).sqrt();
    if c2_len < 1e-9 {
        return false;
    }
    let cross2 = c2_dx * out_dy - c2_dy * out_dx;
    if cross2.abs() / (c2_len * out_len) > 0.05 {
        return false;
    }

    // c1 extends forward, c2 extends backward
    let dot1 = c1_dx * in_dx + c1_dy * in_dy;
    if dot1 < 0.0 {
        return false;
    }
    let dot2 = c2_dx * out_dx + c2_dy * out_dy;
    if dot2 > 0.0 {
        return false;
    }

    true
}

/// Compute intersection of two lines: line1 through (a, b) and line2 through (c, d).
fn line_line_intersection(a: Point2D, b: Point2D, c: Point2D, d: Point2D) -> Option<Point2D> {
    let d1x = b.x - a.x;
    let d1y = b.y - a.y;
    let d2x = d.x - c.x;
    let d2y = d.y - c.y;

    let denom = d1x * d2y - d1y * d2x;
    if denom.abs() < 1e-12 {
        return None; // Parallel lines
    }

    let t = ((c.x - a.x) * d2y - (c.y - a.y) * d2x) / denom;
    Some(Point2D::new(a.x + t * d1x, a.y + t * d1y))
}

/// Apply radius to a single corner of a VecPath identified by subpath_index and vertex_index.
///
/// `vertex_index` matches the indices returned by `get_fillet_candidates`.
/// For already-filleted corners, unfillets first (restores the original sharp corner),
/// then re-fillets with the new radius. If `radius_mm` is ~0, just unfillets.
pub fn apply_radius_at_corner(
    path: &VecPath,
    subpath_index: usize,
    vertex_index: usize,
    radius_mm: f64,
) -> VecPath {
    let mut result_subpaths = path.subpaths.clone();
    if subpath_index >= result_subpaths.len() {
        return path.clone();
    }

    // Check if this corner is already filleted by verifying:
    // 1. The draw command at vertex_index is a CubicTo (first in a contiguous run)
    // 2. The command before the run is a LineTo
    // 3. The command after the run is a LineTo
    // 4. The cubic run is geometrically a fillet arc (tangent or dog-bone)
    let sp = &result_subpaths[subpath_index];
    // Returns Some(corner_point) if the corner is already filleted, None otherwise.
    let already_filleted_corner: Option<Point2D> = {
        let mut draw_cmds: Vec<(Point2D, PathCommand)> = Vec::new();
        let mut current = Point2D::zero();
        for cmd in &sp.commands {
            match *cmd {
                PathCommand::MoveTo { x, y } => {
                    current = Point2D::new(x, y);
                }
                PathCommand::Close => {}
                _ => {
                    let ep = command_endpoint(Some(cmd)).unwrap_or(current);
                    draw_cmds.push((current, *cmd));
                    current = ep;
                }
            }
        }
        let n = draw_cmds.len();
        let is_closed = sp.closed;
        if vertex_index < n && matches!(draw_cmds[vertex_index].1, PathCommand::CubicTo { .. }) {
            let prev_idx = if is_closed {
                (vertex_index + n - 1) % n
            } else if vertex_index > 0 {
                vertex_index - 1
            } else {
                n
            };

            // Scan forward through contiguous CubicTos
            let mut run_end = vertex_index;
            loop {
                let after = if is_closed {
                    (run_end + 1) % n
                } else {
                    if run_end + 1 >= n {
                        break;
                    }
                    run_end + 1
                };
                if matches!(draw_cmds[after].1, PathCommand::CubicTo { .. }) {
                    run_end = after;
                    if run_end == prev_idx {
                        break;
                    }
                } else {
                    break;
                }
            }

            let next_idx = if is_closed {
                (run_end + 1) % n
            } else if run_end + 1 < n {
                run_end + 1
            } else {
                n
            };

            if prev_idx < n
                && next_idx < n
                && matches!(draw_cmds[prev_idx].1, PathCommand::LineTo { .. })
                && matches!(draw_cmds[next_idx].1, PathCommand::LineTo { .. })
            {
                let tangent_in = draw_cmds[vertex_index].0;
                let tangent_out =
                    command_endpoint(Some(&draw_cmds[run_end].1)).unwrap_or(tangent_in);
                let line_in_start = draw_cmds[prev_idx].0;
                let line_out_end =
                    command_endpoint(Some(&draw_cmds[next_idx].1)).unwrap_or(tangent_out);

                // Collect cubics in the run
                let mut cubics = Vec::new();
                let mut idx = vertex_index;
                loop {
                    if let PathCommand::CubicTo {
                        c1x,
                        c1y,
                        c2x,
                        c2y,
                        x,
                        y,
                    } = draw_cmds[idx].1
                    {
                        cubics.push((
                            Point2D::new(c1x, c1y),
                            Point2D::new(c2x, c2y),
                            Point2D::new(x, y),
                        ));
                    }
                    if idx == run_end {
                        break;
                    }
                    idx = if is_closed { (idx + 1) % n } else { idx + 1 };
                }

                let is_fillet = if cubics.len() == 1 {
                    let (c1, c2, _) = cubics[0];
                    is_fillet_cubic(line_in_start, tangent_in, c1, c2, tangent_out, line_out_end)
                } else {
                    is_multi_fillet_cubic(
                        line_in_start,
                        tangent_in,
                        &cubics,
                        tangent_out,
                        line_out_end,
                    )
                };
                if is_fillet {
                    // Compute the original sharp corner position
                    line_line_intersection(line_in_start, tangent_in, tangent_out, line_out_end)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    };

    if let Some(corner_pt) = already_filleted_corner {
        // Unfillet first: replace CubicTo run with the original sharp corner
        result_subpaths[subpath_index] = unfillet_at(&result_subpaths[subpath_index], vertex_index);
        // If radius is ~0, we're just unfilleting
        if radius_mm.abs() < 1e-12 {
            return VecPath {
                subpaths: result_subpaths,
            };
        }
        // Find the restored sharp corner's correct index by position match.
        // After unfillet the draw_cmds array shrank, so vertex_index is stale —
        // use get_fillet_candidates to find the correct index for the restored corner.
        let intermediate = VecPath {
            subpaths: result_subpaths.clone(),
        };
        let candidates = get_fillet_candidates(&intermediate);
        if let Some(c) = candidates.iter().find(|c| {
            c.subpath_index == subpath_index
                && !c.already_filleted
                && (c.x - corner_pt.x).abs() < 1e-6
                && (c.y - corner_pt.y).abs() < 1e-6
        }) {
            result_subpaths[subpath_index] =
                fillet_subpath_at(&result_subpaths[subpath_index], c.vertex_index, radius_mm);
        }
    } else {
        if radius_mm.abs() < 1e-12 {
            return path.clone();
        }
        result_subpaths[subpath_index] =
            fillet_subpath_at(&result_subpaths[subpath_index], vertex_index, radius_mm);
    }

    VecPath {
        subpaths: result_subpaths,
    }
}

/// Unfillet a corner: remove the contiguous CubicTo run starting at `target_draw_idx`
/// and restore a sharp corner computed by intersecting the adjacent line directions.
///
/// Handles both single-cubic fillets (90° corners) and multi-cubic fillets (acute corners
/// where the arc is split into multiple segments).
///
/// `target_draw_idx` is the index into draw commands (excluding MoveTo/Close).
fn unfillet_at(sp: &SubPath, target_draw_idx: usize) -> SubPath {
    // Collect draw commands with their start points
    let mut draw_cmds: Vec<(Point2D, PathCommand)> = Vec::new();
    let mut current = Point2D::zero();

    for cmd in &sp.commands {
        match *cmd {
            PathCommand::MoveTo { x, y } => {
                current = Point2D::new(x, y);
            }
            PathCommand::Close => {}
            _ => {
                let ep = command_endpoint(Some(cmd)).unwrap_or(current);
                draw_cmds.push((current, *cmd));
                current = ep;
            }
        }
    }

    if target_draw_idx >= draw_cmds.len() {
        return sp.clone();
    }

    // The target must be a CubicTo
    if !matches!(draw_cmds[target_draw_idx].1, PathCommand::CubicTo { .. }) {
        return sp.clone();
    }

    let n = draw_cmds.len();
    let is_closed = sp.closed;

    // Scan forward from target to find the end of the contiguous CubicTo run
    let mut run_end = target_draw_idx;
    loop {
        let after = if is_closed {
            (run_end + 1) % n
        } else {
            if run_end + 1 >= n {
                break;
            }
            run_end + 1
        };
        if matches!(draw_cmds[after].1, PathCommand::CubicTo { .. }) {
            run_end = after;
            if run_end == target_draw_idx {
                break;
            } // wrapped all the way around
        } else {
            break;
        }
    }

    // Find the LineTo before the run and the LineTo after the run
    let prev_idx = if is_closed {
        (target_draw_idx + n - 1) % n
    } else {
        if target_draw_idx == 0 {
            return sp.clone();
        }
        target_draw_idx - 1
    };
    let next_idx = if is_closed {
        (run_end + 1) % n
    } else {
        if run_end + 1 >= n {
            return sp.clone();
        }
        run_end + 1
    };

    // Compute the virtual corner by intersecting the two adjacent line directions.
    // tangent_in = start of the first CubicTo in the run
    // tangent_out = endpoint of the last CubicTo in the run
    let tangent_in = draw_cmds[target_draw_idx].0;
    let line_in_start = draw_cmds[prev_idx].0;
    let tangent_out = command_endpoint(Some(&draw_cmds[run_end].1)).unwrap_or(tangent_in);
    let line_out_end = command_endpoint(Some(&draw_cmds[next_idx].1)).unwrap_or(tangent_out);

    let corner = match line_line_intersection(line_in_start, tangent_in, tangent_out, line_out_end)
    {
        Some(c) => c,
        None => return sp.clone(), // Can't determine corner — bail
    };

    // Collect the set of draw indices to skip (the whole cubic run)
    let mut skip_indices = std::collections::HashSet::new();
    {
        let mut idx = target_draw_idx;
        loop {
            skip_indices.insert(idx);
            if idx == run_end {
                break;
            }
            idx = if is_closed { (idx + 1) % n } else { idx + 1 };
        }
    }

    // Check if this is the closing corner (run includes the last draw cmd in a closed path).
    let is_closing_corner = is_closed && skip_indices.contains(&(n - 1));

    // Rebuild: skip all CubicTos in the run, extend the preceding LineTo to the corner,
    // and for closing corners, reset the MoveTo.
    let mut result_cmds: Vec<PathCommand> = Vec::new();
    let mut draw_idx = 0;

    for cmd in &sp.commands {
        match cmd {
            PathCommand::MoveTo { .. } => {
                if is_closing_corner {
                    result_cmds.push(PathCommand::MoveTo {
                        x: corner.x,
                        y: corner.y,
                    });
                } else {
                    result_cmds.push(*cmd);
                }
            }
            PathCommand::Close => {
                result_cmds.push(PathCommand::Close);
            }
            _ => {
                if skip_indices.contains(&draw_idx) {
                    // Skip all CubicTos in the fillet run
                    draw_idx += 1;
                    continue;
                }
                if draw_idx == prev_idx {
                    // Extend preceding LineTo to end at the restored corner
                    if matches!(cmd, PathCommand::LineTo { .. }) {
                        result_cmds.push(PathCommand::LineTo {
                            x: corner.x,
                            y: corner.y,
                        });
                    } else {
                        result_cmds.push(*cmd);
                    }
                } else {
                    result_cmds.push(*cmd);
                }
                draw_idx += 1;
            }
        }
    }

    SubPath {
        commands: result_cmds,
        closed: sp.closed,
    }
}

/// Fillet a single corner (at `target_idx`) in a subpath.
fn fillet_subpath_at(sp: &SubPath, target_idx: usize, radius: f64) -> SubPath {
    let mut draw_cmds: Vec<(Point2D, PathCommand)> = Vec::new();
    let mut move_pt = Point2D::zero();
    let mut current = Point2D::zero();

    for cmd in &sp.commands {
        match *cmd {
            PathCommand::MoveTo { x, y } => {
                move_pt = Point2D::new(x, y);
                current = move_pt;
            }
            PathCommand::Close => {}
            _ => {
                let ep = command_endpoint(Some(cmd)).unwrap_or(current);
                draw_cmds.push((current, *cmd));
                current = ep;
            }
        }
    }

    if draw_cmds.len() < 2 {
        return sp.clone();
    }

    let n = draw_cmds.len();
    let is_closed = sp.closed;

    let closing_edge: Option<(Point2D, PathCommand)> = if is_closed {
        let last_ep = command_endpoint(Some(&draw_cmds[n - 1].1)).unwrap_or(current);
        if last_ep.distance_to(&move_pt) > 1e-9 {
            Some((
                last_ep,
                PathCommand::LineTo {
                    x: move_pt.x,
                    y: move_pt.y,
                },
            ))
        } else {
            None
        }
    } else {
        None
    };

    let full_len = n + if closing_edge.is_some() { 1 } else { 0 };
    let mut all_cmds: Vec<(Point2D, PathCommand)> = draw_cmds.clone();
    if let Some(ce) = closing_edge {
        all_cmds.push(ce);
    }

    // Only compute fillet for the target corner
    let mut fillet_info: Vec<Option<FilletArc>> = vec![None; full_len];

    if target_idx < full_len {
        let i = target_idx;
        // For open paths, first vertex has no incoming edge
        if i == 0 && !is_closed {
            return sp.clone();
        }
        let next_idx = if is_closed {
            (i + 1) % full_len
        } else {
            if i + 1 >= full_len {
                return sp.clone();
            }
            i + 1
        };

        let curr_is_line = matches!(all_cmds[i].1, PathCommand::LineTo { .. });
        let next_is_line = matches!(all_cmds[next_idx].1, PathCommand::LineTo { .. });

        if curr_is_line && next_is_line {
            let corner_pt = command_endpoint(Some(&all_cmds[i].1)).unwrap_or(all_cmds[i].0);
            let prev_pt = all_cmds[i].0;
            let next_ep = command_endpoint(Some(&all_cmds[next_idx].1)).unwrap_or(corner_pt);
            fillet_info[i] = compute_fillet_arc(prev_pt, corner_pt, next_ep, radius);
        }
    }

    // Emit — reuses same logic as fillet_subpath
    let mut result_cmds: Vec<PathCommand> = vec![PathCommand::MoveTo {
        x: move_pt.x,
        y: move_pt.y,
    }];

    if is_closed
        && full_len >= 2
        && let Some(ref arc) = fillet_info[full_len - 1]
    {
        result_cmds[0] = PathCommand::MoveTo {
            x: arc.tangent_out.x,
            y: arc.tangent_out.y,
        };
    }

    for i in 0..n {
        let cmd = &all_cmds[i];
        if let Some(ref arc) = fillet_info[i] {
            emit_shortened_cmd(&mut result_cmds, &cmd.1, arc.tangent_in);
            emit_fillet_cubics(&mut result_cmds, arc);
        } else {
            result_cmds.push(cmd.1);
        }
    }

    if is_closed {
        if let Some(ce) = closing_edge {
            let ce_idx = n;
            if let Some(ref arc) = fillet_info.get(ce_idx).and_then(|f| f.clone()) {
                emit_shortened_cmd(&mut result_cmds, &ce.1, arc.tangent_in);
                emit_fillet_cubics(&mut result_cmds, arc);
            } else {
                result_cmds.push(ce.1);
            }
        }
        result_cmds.push(PathCommand::Close);
    }

    SubPath {
        commands: result_cmds,
        closed: sp.closed,
    }
}

/// Apply radius (fillet) to corners of a path using arc approximation.
///
/// Only fillets LineTo→LineTo corners, preserving existing CubicTo/QuadTo curves.
/// Negative radius creates concave bites. Zero radius is a no-op.
pub fn apply_radius(path: &VecPath, radius_mm: f64) -> VecPath {
    if radius_mm.abs() < 1e-12 {
        return path.clone();
    }

    let mut result_subpaths = Vec::new();
    for sp in &path.subpaths {
        result_subpaths.push(fillet_subpath(sp, radius_mm));
    }
    VecPath {
        subpaths: result_subpaths,
    }
}

/// Fillet a single subpath at LineTo→LineTo corners.
fn fillet_subpath(sp: &SubPath, radius: f64) -> SubPath {
    // Collect (endpoint, command) pairs for draw commands
    let mut draw_cmds: Vec<(Point2D, PathCommand)> = Vec::new();
    let mut move_pt = Point2D::zero();
    let mut current = Point2D::zero();

    for cmd in &sp.commands {
        match *cmd {
            PathCommand::MoveTo { x, y } => {
                move_pt = Point2D::new(x, y);
                current = move_pt;
            }
            PathCommand::Close => {}
            _ => {
                let ep = command_endpoint(Some(cmd)).unwrap_or(current);
                draw_cmds.push((current, *cmd));
                current = ep;
            }
        }
    }

    if draw_cmds.len() < 2 {
        return sp.clone();
    }

    let n = draw_cmds.len();
    let is_closed = sp.closed;

    // For closed paths, include the closing edge as a virtual LineTo
    // from last vertex back to move_pt. This creates corners at both
    // the first and last vertices.
    let closing_edge: Option<(Point2D, PathCommand)> = if is_closed {
        let last_ep = command_endpoint(Some(&draw_cmds[n - 1].1)).unwrap_or(current);
        if last_ep.distance_to(&move_pt) > 1e-9 {
            Some((
                last_ep,
                PathCommand::LineTo {
                    x: move_pt.x,
                    y: move_pt.y,
                },
            ))
        } else {
            None
        }
    } else {
        None
    };

    // Build full circular sequence for corner detection:
    // For closed paths: draw_cmds + closing_edge (if any), treated as circular
    let full_len = n + if closing_edge.is_some() { 1 } else { 0 };

    // Build full command list for corner analysis
    let mut all_cmds: Vec<(Point2D, PathCommand)> = draw_cmds.clone();
    if let Some(ce) = closing_edge {
        all_cmds.push(ce);
    }

    // Determine which corners are filleted (adjacent commands both LineTo)
    let mut fillet_info: Vec<Option<FilletArc>> = vec![None; full_len];

    let corner_count = if is_closed { full_len } else { full_len };
    for i in 0..corner_count {
        // Corner at the endpoint of command i
        let prev_idx = if i == 0 {
            if is_closed {
                full_len - 1
            } else {
                continue;
            }
        } else {
            i - 1
        };
        let next_idx = if is_closed {
            (i + 1) % full_len
        } else {
            if i + 1 >= full_len {
                continue;
            }
            i + 1
        };

        // Both commands must be LineTo (or the virtual closing LineTo)
        let _prev_is_line = matches!(all_cmds[prev_idx].1, PathCommand::LineTo { .. });
        let curr_is_line = matches!(all_cmds[i].1, PathCommand::LineTo { .. });
        let next_is_line = matches!(all_cmds[next_idx].1, PathCommand::LineTo { .. });

        // Corner at the endpoint of command[i] = start of command[i+1]
        // Incoming = command[i], outgoing = command[i+1] (next_idx)
        if !curr_is_line || !next_is_line {
            continue;
        }
        // For the first vertex of a closed path (corner between closing edge and first draw cmd),
        // the incoming is prev_idx and the outgoing is i (index 0)
        // Actually: corner at the shared vertex between cmd[i] endpoint and cmd[next_idx] start
        let corner_pt = command_endpoint(Some(&all_cmds[i].1)).unwrap_or(all_cmds[i].0);
        let prev_pt = all_cmds[i].0;
        let next_ep = command_endpoint(Some(&all_cmds[next_idx].1)).unwrap_or(corner_pt);

        if let Some(arc) = compute_fillet_arc(prev_pt, corner_pt, next_ep, radius) {
            fillet_info[i] = Some(arc);
        }
    }

    // Now emit commands with fillets inserted
    let mut result_cmds: Vec<PathCommand> = vec![PathCommand::MoveTo {
        x: move_pt.x,
        y: move_pt.y,
    }];

    // For closed paths, if the corner at vertex 0 is filleted, adjust MoveTo
    // to the fillet's tangent_out (on the first draw edge).
    // The fillet is at fillet_info[full_len - 1] (last entry = closing edge or last draw cmd).
    if is_closed
        && full_len >= 2
        && let Some(ref arc) = fillet_info[full_len - 1]
    {
        result_cmds[0] = PathCommand::MoveTo {
            x: arc.tangent_out.x,
            y: arc.tangent_out.y,
        };
    }

    for i in 0..n {
        let cmd = &all_cmds[i];
        let _ep = command_endpoint(Some(&cmd.1)).unwrap_or(cmd.0);

        if let Some(ref arc) = fillet_info[i] {
            // Shorten this command to end at tangent_in
            emit_shortened_cmd(&mut result_cmds, &cmd.1, arc.tangent_in);
            // Emit the arc (one or more cubic segments)
            emit_fillet_cubics(&mut result_cmds, arc);
        } else {
            result_cmds.push(cmd.1);
        }
    }

    // Handle closing edge
    if is_closed {
        if let Some(ce) = closing_edge {
            let ce_idx = n; // index in all_cmds
            if let Some(ref arc) = fillet_info.get(ce_idx).and_then(|f| f.clone()) {
                emit_shortened_cmd(&mut result_cmds, &ce.1, arc.tangent_in);
                emit_fillet_cubics(&mut result_cmds, arc);
            } else {
                // Emit closing edge explicitly before Close
                result_cmds.push(ce.1);
            }
        }

        result_cmds.push(PathCommand::Close);
    }

    SubPath {
        commands: result_cmds,
        closed: sp.closed,
    }
}

#[derive(Debug, Clone)]
struct FilletArc {
    tangent_in: Point2D,
    tangent_out: Point2D,
    /// One or more cubic Bezier segments approximating the arc.
    /// Each entry is (ctrl1, ctrl2, endpoint).
    cubics: Vec<(Point2D, Point2D, Point2D)>,
}

/// Push all cubic Bezier segments of a fillet arc into a command list.
fn emit_fillet_cubics(cmds: &mut Vec<PathCommand>, arc: &FilletArc) {
    for (ctrl1, ctrl2, ep) in &arc.cubics {
        cmds.push(PathCommand::CubicTo {
            c1x: ctrl1.x,
            c1y: ctrl1.y,
            c2x: ctrl2.x,
            c2y: ctrl2.y,
            x: ep.x,
            y: ep.y,
        });
    }
}

/// Compute arc fillet parameters for a corner at `corner` between edges from `prev` and to `next`.
///
/// Positive radius: convex fillet (short arc, single cubic for ≤90° corners).
/// Negative radius: concave bite (reflex arc, multi-segment cubics).
fn compute_fillet_arc(
    prev: Point2D,
    corner: Point2D,
    next: Point2D,
    radius: f64,
) -> Option<FilletArc> {
    let d_in_x = corner.x - prev.x;
    let d_in_y = corner.y - prev.y;
    let len_in = (d_in_x * d_in_x + d_in_y * d_in_y).sqrt();

    let d_out_x = next.x - corner.x;
    let d_out_y = next.y - corner.y;
    let len_out = (d_out_x * d_out_x + d_out_y * d_out_y).sqrt();

    if len_in < 1e-9 || len_out < 1e-9 {
        return None;
    }

    // Unit vectors
    let u_in_x = d_in_x / len_in;
    let u_in_y = d_in_y / len_in;
    let u_out_x = d_out_x / len_out;
    let u_out_y = d_out_y / len_out;

    // Interior corner angle between the corner->prev and corner->next rays.
    // Using the turn/exterior angle here makes acute corners too small and
    // wider corners too large for the same requested fillet radius.
    let dot = (-u_in_x) * u_out_x + (-u_in_y) * u_out_y;
    let dot_clamped = dot.clamp(-1.0, 1.0);
    let angle = dot_clamped.acos(); // angle between edges (0..π)

    if angle < 1e-6 || (std::f64::consts::PI - angle) < 1e-6 {
        return None; // Collinear or folded
    }

    let half_angle = angle / 2.0;
    let abs_radius = radius.abs();

    // Tangent distance from corner along each edge
    let tan_dist = abs_radius / (half_angle).tan();
    // Clamp to half edge length
    let tan_dist = tan_dist.min(len_in / 2.0).min(len_out / 2.0);
    let effective_radius = tan_dist * (half_angle).tan();

    if effective_radius < 1e-9 {
        return None;
    }

    // Tangent points (same for positive and negative radius)
    let tangent_in = Point2D::new(corner.x - u_in_x * tan_dist, corner.y - u_in_y * tan_dist);
    let tangent_out = Point2D::new(corner.x + u_out_x * tan_dist, corner.y + u_out_y * tan_dist);

    // Choose arc center and radius based on fillet type.
    //
    // Positive radius: standard convex fillet. Center at the intersection of
    // interior perpendiculars from both edges (inside the corner angle).
    // Arc is tangent to both edges at the tangent points.
    //
    // Negative radius: dog-bone relief. Center at the corner vertex itself.
    // Arc passes through both tangent points but is NOT tangent to the edges
    // (intentional — creates a concave notch for CNC corner clearance).
    let cross = u_in_x * u_out_y - u_in_y * u_out_x;
    if cross.abs() < 1e-9 {
        return None;
    }

    let (cx, cy, arc_radius) = if radius >= 0.0 {
        let turn_sign = if cross > 0.0 { 1.0 } else { -1.0 };
        let n_in_x = -turn_sign * u_in_y;
        let n_in_y = turn_sign * u_in_x;
        (
            tangent_in.x + effective_radius * n_in_x,
            tangent_in.y + effective_radius * n_in_y,
            effective_radius,
        )
    } else {
        // Dog-bone: center at corner, radius = distance from corner to tangent points
        (corner.x, corner.y, tan_dist)
    };

    // Start and end angles on the circle
    let start_angle = (tangent_in.y - cy).atan2(tangent_in.x - cx);
    let end_angle = (tangent_out.y - cy).atan2(tangent_out.x - cx);

    // Short sweep normalized to (-π, π]
    let mut sweep = end_angle - start_angle;
    if sweep > std::f64::consts::PI {
        sweep -= 2.0 * std::f64::consts::PI;
    }
    if sweep <= -std::f64::consts::PI {
        sweep += 2.0 * std::f64::consts::PI;
    }

    // Split into segments (max π/2 each for accurate cubic approximation).
    // Subtract tiny epsilon before ceil to avoid floating-point rounding giving
    // an extra segment (e.g. π/2 + ε → ceil(1+δ) = 2 instead of 1).
    let n_segs = (((sweep.abs() / std::f64::consts::FRAC_PI_2) - 1e-9)
        .ceil()
        .max(1.0) as usize)
        .max(1);
    let seg_sweep = sweep / n_segs as f64;

    let mut cubics = Vec::with_capacity(n_segs);
    for s in 0..n_segs {
        let theta1 = start_angle + s as f64 * seg_sweep;
        let theta2 = start_angle + (s + 1) as f64 * seg_sweep;

        let alpha = (4.0 / 3.0) * (seg_sweep / 4.0).tan();

        let p1x = cx + arc_radius * theta1.cos();
        let p1y = cy + arc_radius * theta1.sin();
        let p2x = cx + arc_radius * theta2.cos();
        let p2y = cy + arc_radius * theta2.sin();

        let c1 = Point2D::new(
            p1x + alpha * arc_radius * (-theta1.sin()),
            p1y + alpha * arc_radius * theta1.cos(),
        );
        let c2 = Point2D::new(
            p2x - alpha * arc_radius * (-theta2.sin()),
            p2y - alpha * arc_radius * theta2.cos(),
        );
        let ep = Point2D::new(p2x, p2y);

        cubics.push((c1, c2, ep));
    }

    // Force last endpoint to be exactly tangent_out (avoid floating-point drift)
    if let Some(last) = cubics.last_mut() {
        last.2 = tangent_out;
    }

    Some(FilletArc {
        tangent_in,
        tangent_out,
        cubics,
    })
}

/// Emit a shortened version of a LineTo command ending at `new_endpoint`.
fn emit_shortened_cmd(cmds: &mut Vec<PathCommand>, _original: &PathCommand, new_endpoint: Point2D) {
    cmds.push(PathCommand::LineTo {
        x: new_endpoint.x,
        y: new_endpoint.y,
    });
}

/// Result of close_and_join operation.
pub struct CloseJoinResult {
    pub path: VecPath,
    /// True iff every SubPath in the result is closed.
    pub fully_closed: bool,
}

/// Join open subpaths by endpoint matching and close loops within tolerance.
///
/// Output contract:
/// - Returns `CloseJoinResult` with one VecPath containing one or more SubPaths.
/// - `fully_closed: bool` — true only if ALL subpaths in the result are closed.
/// - Already-closed input subpaths pass through as-is.
/// - Open subpaths are chained by endpoint matching; chains that loop within
///   tolerance get PathCommand::Close.
/// - Chains that do NOT close remain as open SubPaths (not discarded).
pub fn close_and_join(paths: &[VecPath], tolerance: f64) -> CloseJoinResult {
    use crate::vector::trim::{reverse_subpath, subpath_first_point, subpath_last_point};

    let mut open_sps: Vec<SubPath> = Vec::new();
    let mut closed_sps: Vec<SubPath> = Vec::new();

    for vp in paths {
        for sp in &vp.subpaths {
            if sp.closed {
                closed_sps.push(sp.clone());
            } else if sp.commands.len() > 1 {
                open_sps.push(sp.clone());
            }
        }
    }

    let mut result_sps: Vec<SubPath> = Vec::new();

    // Pass-through closed subpaths
    result_sps.extend(closed_sps);

    // Build chains from open subpaths using greedy endpoint matching
    let mut remaining = open_sps;

    while !remaining.is_empty() {
        let mut chain = vec![remaining.remove(0)];

        loop {
            let chain_end = subpath_last_point(chain.last().unwrap());
            let Some(chain_end) = chain_end else { break };

            let mut best_idx = None;
            let mut best_reversed = false;
            let mut best_dist = f64::INFINITY;

            for (i, sp) in remaining.iter().enumerate() {
                let sp_start = subpath_first_point(sp);
                let sp_end = subpath_last_point(sp);

                if let Some(start) = sp_start {
                    let d = chain_end.distance_to(&start);
                    if d < tolerance && d < best_dist {
                        best_dist = d;
                        best_idx = Some(i);
                        best_reversed = false;
                    }
                }
                if let Some(end) = sp_end {
                    let d = chain_end.distance_to(&end);
                    if d < tolerance && d < best_dist {
                        best_dist = d;
                        best_idx = Some(i);
                        best_reversed = true;
                    }
                }
            }

            if let Some(idx) = best_idx {
                let sp = remaining.remove(idx);
                let sp = if best_reversed {
                    reverse_subpath(&sp)
                } else {
                    sp
                };
                chain.push(sp);
            } else {
                break;
            }
        }

        // Also try extending from the front of the chain
        loop {
            let chain_start = subpath_first_point(chain.first().unwrap());
            let Some(chain_start) = chain_start else {
                break;
            };

            let mut best_idx = None;
            let mut best_reversed = false;
            let mut best_dist = f64::INFINITY;

            for (i, sp) in remaining.iter().enumerate() {
                let sp_start = subpath_first_point(sp);
                let sp_end = subpath_last_point(sp);

                // We want sp's END to match chain's START
                if let Some(end) = sp_end {
                    let d = chain_start.distance_to(&end);
                    if d < tolerance && d < best_dist {
                        best_dist = d;
                        best_idx = Some(i);
                        best_reversed = false;
                    }
                }
                if let Some(start) = sp_start {
                    let d = chain_start.distance_to(&start);
                    if d < tolerance && d < best_dist {
                        best_dist = d;
                        best_idx = Some(i);
                        best_reversed = true;
                    }
                }
            }

            if let Some(idx) = best_idx {
                let sp = remaining.remove(idx);
                let sp = if best_reversed {
                    reverse_subpath(&sp)
                } else {
                    sp
                };
                chain.insert(0, sp);
            } else {
                break;
            }
        }

        // Merge chain into a single SubPath
        let merged = merge_chain(&chain);

        // Check if start ≈ end → close it
        let start = subpath_first_point(&merged);
        let end = subpath_last_point(&merged);
        let should_close = if let (Some(s), Some(e)) = (start, end) {
            s.distance_to(&e) < tolerance
        } else {
            false
        };

        if should_close {
            let mut cmds = merged.commands;
            cmds.push(PathCommand::Close);
            result_sps.push(SubPath {
                commands: cmds,
                closed: true,
            });
        } else {
            result_sps.push(merged);
        }
    }

    let fully_closed = result_sps.iter().all(|sp| sp.closed);

    CloseJoinResult {
        path: VecPath {
            subpaths: result_sps,
        },
        fully_closed,
    }
}

/// Merge a chain of SubPaths into one SubPath by concatenating draw commands.
fn merge_chain(chain: &[SubPath]) -> SubPath {
    if chain.is_empty() {
        return SubPath {
            commands: vec![],
            closed: false,
        };
    }

    let mut commands = Vec::new();
    for (i, sp) in chain.iter().enumerate() {
        for (j, cmd) in sp.commands.iter().enumerate() {
            match *cmd {
                PathCommand::MoveTo { x, y } => {
                    if i == 0 && j == 0 {
                        commands.push(PathCommand::MoveTo { x, y });
                    }
                }
                PathCommand::Close => {}
                _ => {
                    commands.push(*cmd);
                }
            }
        }
    }

    SubPath {
        commands,
        closed: false,
    }
}

/// Rotate a closed path so the start point is nearest to (x, y).
/// Now uses segment-based rotation to preserve curves.
pub fn set_start_point(path: &VecPath, x: f64, y: f64) -> VecPath {
    let target = Point2D { x, y };
    let vertices = get_path_vertices(path);

    // Find nearest closed-subpath vertex
    let mut nearest: Option<(usize, usize, f64)> = None;
    for v in &vertices {
        if !v.subpath_closed {
            continue;
        }
        let dx = v.x - target.x;
        let dy = v.y - target.y;
        let dist_sq = dx * dx + dy * dy;
        if nearest.is_none() || dist_sq < nearest.unwrap().2 {
            nearest = Some((v.subpath_index, v.vertex_index, dist_sq));
        }
    }

    let Some((sp_idx, v_idx, _)) = nearest else {
        return path.clone();
    };

    // Normalize, then rotate
    let mut vp = path.clone();
    let (norm_sp, _) = normalize_closed_subpath(&vp.subpaths[sp_idx]);
    vp.subpaths[sp_idx] = norm_sp;
    rotate_subpath_start(&vp, sp_idx, v_idx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{ObjectData, ShapeKind};
    use beambench_common::{Bounds, Point2D, Transform2D};

    fn assert_near(actual: f64, expected: f64, tol: f64, label: &str) {
        assert!(
            (actual - expected).abs() <= tol,
            "{label}: expected {expected:.6}, got {actual:.6}",
        );
    }

    #[test]
    fn close_path_adds_z() {
        let result = close_path("M0 0 L10 0 L10 10 L0 10");
        assert!(result.ends_with('Z'));
    }

    #[test]
    fn close_path_already_closed() {
        let input = "M0 0 L10 0 L10 10 L0 10 Z";
        let result = close_path(input);
        assert_eq!(result, input);
    }

    #[test]
    fn close_paths_with_tolerance_closes_near_endpoints() {
        // Path ends at (0.1, 0.1), starts at (0, 0) - distance ~0.14
        let paths = vec!["M0 0 L10 0 L10 10 L0 10 L0.1 0.1".to_string()];
        let result = close_paths_with_tolerance(&paths, 0.5);

        assert_eq!(result.len(), 1);
        assert!(result[0].contains('Z') || result[0].contains('z'));
    }

    #[test]
    fn close_paths_with_tolerance_leaves_open() {
        let paths = vec!["M0 0 L10 0 L10 10 L5 10".to_string()];
        let result = close_paths_with_tolerance(&paths, 0.5);

        assert_eq!(result.len(), 1);
        // Far endpoints should not be closed
        assert!(!result[0].ends_with('Z') && !result[0].ends_with('z'));
    }

    #[test]
    fn auto_join_connects_paths() {
        let a = VecPath::parse_svg_d("M0 0 L10 0");
        let b = VecPath::parse_svg_d("M10 0 L20 0");

        let result = auto_join_paths(&[a, b], 0.1);

        // Should join into one path
        assert_eq!(result.len(), 1);
        assert!(result[0].subpaths.len() >= 2);
    }

    #[test]
    fn auto_join_keeps_separate_if_far() {
        let a = VecPath::parse_svg_d("M0 0 L10 0");
        let b = VecPath::parse_svg_d("M20 0 L30 0");

        let result = auto_join_paths(&[a, b], 0.1);

        // Should remain separate
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn optimize_path_simplifies() {
        let path = VecPath::parse_svg_d("M0 0 L1 0 L2 0 L3 0 L4 0 L5 0");
        let result = optimize_path(&path, 1.0);

        assert!(!result.is_empty());
        // Should have fewer points than original
        let original_point_count = 6;
        let result_point_count: usize = result.subpaths.iter().map(|sp| sp.commands.len()).sum();
        assert!(result_point_count <= original_point_count);
    }

    #[test]
    fn find_duplicates_detects_same_objects() {
        use crate::layer::LayerId;

        let layer_id = LayerId::new();

        let obj1 = ProjectObject {
            id: ObjectId::new(),
            name: "rect1".into(),
            visible: true,
            locked: false,
            bounds: Bounds {
                min: Point2D { x: 0.0, y: 0.0 },
                max: Point2D { x: 10.0, y: 10.0 },
            },
            transform: Transform2D::identity(),
            layer_id,
            data: ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
            z_index: 0,
            lock_aspect_ratio: false,
            power_scale: 1.0,
            priority: 0,
            created_at: chrono::Utc::now(),
            tabs: Vec::new(),
            start_point_edits: Vec::new(),
        };

        let obj2 = ProjectObject {
            id: ObjectId::new(),
            name: "rect2".into(),
            visible: true,
            locked: false,
            bounds: Bounds {
                min: Point2D { x: 0.0, y: 0.0 },
                max: Point2D { x: 10.0, y: 10.0 },
            },
            transform: Transform2D::identity(),
            layer_id,
            data: ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
            z_index: 0,
            lock_aspect_ratio: false,
            power_scale: 1.0,
            priority: 0,
            created_at: chrono::Utc::now(),
            tabs: Vec::new(),
            start_point_edits: Vec::new(),
        };

        let duplicates = find_duplicates(&[obj1.clone(), obj2.clone()]);
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0], obj2.id);
    }

    #[test]
    fn find_duplicates_different_objects() {
        use crate::layer::LayerId;

        let layer_id = LayerId::new();

        let obj1 = ProjectObject {
            id: ObjectId::new(),
            name: "rect1".into(),
            visible: true,
            locked: false,
            bounds: Bounds {
                min: Point2D { x: 0.0, y: 0.0 },
                max: Point2D { x: 10.0, y: 10.0 },
            },
            transform: Transform2D::identity(),
            layer_id,
            data: ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
            z_index: 0,
            lock_aspect_ratio: false,
            power_scale: 1.0,
            priority: 0,
            created_at: chrono::Utc::now(),
            tabs: Vec::new(),
            start_point_edits: Vec::new(),
        };

        let obj2 = ProjectObject {
            id: ObjectId::new(),
            name: "rect2".into(),
            visible: true,
            locked: false,
            bounds: Bounds {
                min: Point2D { x: 0.0, y: 0.0 },
                max: Point2D { x: 20.0, y: 20.0 },
            },
            transform: Transform2D::identity(),
            layer_id,
            data: ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 20.0,
                height: 20.0,
                corner_radius: 0.0,
            },
            z_index: 0,
            lock_aspect_ratio: false,
            power_scale: 1.0,
            priority: 0,
            created_at: chrono::Utc::now(),
            tabs: Vec::new(),
            start_point_edits: Vec::new(),
        };

        let duplicates = find_duplicates(&[obj1, obj2]);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn break_apart_splits_subpaths() {
        let multi = "M0 0 L10 0 L10 10 Z M20 0 L30 0 L30 10 Z";
        let result = break_apart(multi);

        assert_eq!(result.len(), 2);
    }

    #[test]
    fn apply_radius_fillets_corners() {
        let rect = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let result = apply_radius(&rect, 2.0);

        assert!(!result.is_empty());
        // Should have CubicTo commands from arc fillets
        let has_cubic = result.subpaths.iter().any(|sp| {
            sp.commands
                .iter()
                .any(|c| matches!(c, PathCommand::CubicTo { .. }))
        });
        assert!(has_cubic, "Fillet should produce CubicTo arc segments");
    }

    #[test]
    fn apply_radius_zero_radius() {
        let rect = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let result = apply_radius(&rect, 0.0);

        assert!(!result.is_empty());
    }

    #[test]
    fn apply_radius_negative_creates_concave() {
        let rect = VecPath::parse_svg_d("M0 0 L20 0 L20 20 L0 20 Z");
        let result = apply_radius(&rect, -3.0);
        let has_cubic = result.subpaths.iter().any(|sp| {
            sp.commands
                .iter()
                .any(|c| matches!(c, PathCommand::CubicTo { .. }))
        });
        assert!(
            has_cubic,
            "Negative radius should produce concave CubicTo arcs"
        );
    }

    #[test]
    fn apply_radius_equilateral_triangle_symmetric() {
        let side = 80.0_f64;
        let height = side * (3.0_f64).sqrt() / 2.0;
        let prev = Point2D::new(0.0, 0.0);
        let corner = Point2D::new(side / 2.0, height);
        let next = Point2D::new(side, 0.0);

        let arc = compute_fillet_arc(prev, corner, next, 5.0).expect("expected fillet arc");
        let tangent_in_dist = arc.tangent_in.distance_to(&corner);
        let tangent_out_dist = arc.tangent_out.distance_to(&corner);
        let expected = 5.0 / (std::f64::consts::PI / 6.0).tan();

        assert_near(
            tangent_in_dist,
            tangent_out_dist,
            1e-6,
            "equilateral tangent symmetry",
        );
        assert_near(
            tangent_in_dist,
            expected,
            1e-6,
            "equilateral tangent cutback",
        );

        let tri = VecPath::parse_svg_d(&format!("M0 0 L{side} 0 L{} {} Z", side / 2.0, height));
        let result = apply_radius(&tri, 5.0);
        let cubic_count = result.subpaths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, PathCommand::CubicTo { .. }))
            .count();
        assert_eq!(
            cubic_count, 6,
            "Equilateral triangle should fillet all 3 corners with two cubics per 60° corner"
        );
    }

    #[test]
    fn apply_radius_isosceles_triangle_proportional() {
        let left = Point2D::new(0.0, 0.0);
        let top = Point2D::new(50.0, 150.0);
        let right = Point2D::new(100.0, 0.0);

        let top_arc = compute_fillet_arc(left, top, right, 5.0).expect("expected apex fillet");
        let left_arc = compute_fillet_arc(right, left, top, 5.0).expect("expected base fillet");
        let right_arc = compute_fillet_arc(top, right, left, 5.0).expect("expected base fillet");

        let top_cutback = top_arc.tangent_in.distance_to(&top);
        let left_cutback = left_arc.tangent_in.distance_to(&left);
        let right_cutback = right_arc.tangent_in.distance_to(&right);

        assert!(
            top_cutback > left_cutback,
            "Sharper apex should have larger cutback than base corner, got apex={top_cutback:.6}, base={left_cutback:.6}",
        );
        assert_near(left_cutback, right_cutback, 1e-6, "isosceles base symmetry");
    }

    #[test]
    fn apply_radius_triangle_negative_dogbone() {
        let tri = VecPath::parse_svg_d("M0 0 L80 0 L40 69.2820323027551 Z");
        let result = apply_radius(&tri, -5.0);
        let candidates = get_fillet_candidates(&result);
        let already_filleted = candidates.iter().filter(|c| c.already_filleted).count();
        assert_eq!(
            candidates.len(),
            3,
            "Triangle should still expose 3 radius candidates"
        );
        assert_eq!(
            already_filleted, 3,
            "Negative radius should dog-bone all 3 triangle corners"
        );
    }

    #[test]
    fn compute_fillet_arc_90_degrees_unchanged() {
        let arc = compute_fillet_arc(
            Point2D::new(0.0, 0.0),
            Point2D::new(20.0, 0.0),
            Point2D::new(20.0, 20.0),
            3.0,
        )
        .expect("expected 90-degree fillet");

        assert_near(arc.tangent_in.x, 17.0, 1e-6, "90-degree tangent_in.x");
        assert_near(arc.tangent_in.y, 0.0, 1e-6, "90-degree tangent_in.y");
        assert_near(arc.tangent_out.x, 20.0, 1e-6, "90-degree tangent_out.x");
        assert_near(arc.tangent_out.y, 3.0, 1e-6, "90-degree tangent_out.y");
        assert_eq!(
            arc.cubics.len(),
            1,
            "90-degree fillet should remain a single cubic segment"
        );
    }

    #[test]
    fn triangle_fillet_detect_unfillet_refillet_roundtrip() {
        // Equilateral triangle with large enough edges for a 5mm fillet
        let side = 80.0_f64;
        let height = side * (3.0_f64).sqrt() / 2.0;
        let tri_d = format!("M0 0 L{side} 0 L{} {} Z", side / 2.0, height);
        let tri = VecPath::parse_svg_d(&tri_d);

        // 1. Fillet one corner (vertex 0 = bottom-right corner at (80, 0))
        let candidates = get_fillet_candidates(&tri);
        assert_eq!(candidates.len(), 3, "Triangle should have 3 corners");
        assert!(
            candidates.iter().all(|c| !c.already_filleted),
            "All corners should be fresh"
        );

        let filleted = apply_radius_at_corner(
            &tri,
            candidates[0].subpath_index,
            candidates[0].vertex_index,
            5.0,
        );

        // 2. Confirm get_fillet_candidates reports that corner as already_filleted
        let candidates2 = get_fillet_candidates(&filleted);
        let already_count = candidates2.iter().filter(|c| c.already_filleted).count();
        assert_eq!(
            already_count, 1,
            "One triangle corner should be detected as already filleted, got {already_count}"
        );

        // 3. Unfillet with radius 0
        let filleted_candidate = candidates2.iter().find(|c| c.already_filleted).unwrap();
        let unfilleted = apply_radius_at_corner(
            &filleted,
            filleted_candidate.subpath_index,
            filleted_candidate.vertex_index,
            0.0,
        );

        // 4. No cubics should remain for that corner
        let cubic_count = unfilleted.subpaths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, PathCommand::CubicTo { .. }))
            .count();
        assert_eq!(cubic_count, 0, "Unfilleted triangle should have no cubics");

        // 5. Refillet with different radius
        let candidates3 = get_fillet_candidates(&unfilleted);
        let sharp_count = candidates3.iter().filter(|c| !c.already_filleted).count();
        assert_eq!(sharp_count, 3, "All corners should be sharp again");

        let refilleted = apply_radius_at_corner(
            &unfilleted,
            candidates3[0].subpath_index,
            candidates3[0].vertex_index,
            3.0,
        );
        let recheck = get_fillet_candidates(&refilleted);
        let already_again = recheck.iter().filter(|c| c.already_filleted).count();
        assert_eq!(
            already_again, 1,
            "Refilleted corner should be detected, got {already_again}"
        );
    }

    #[test]
    fn triangle_fillet_all_corners_detect_and_unfillet() {
        // Fillet all 3 corners of a triangle, verify all detected, unfillet all
        let side = 80.0_f64;
        let height = side * (3.0_f64).sqrt() / 2.0;
        let tri_d = format!("M0 0 L{side} 0 L{} {} Z", side / 2.0, height);
        let tri = VecPath::parse_svg_d(&tri_d);

        let filleted = apply_radius(&tri, 5.0);
        let candidates = get_fillet_candidates(&filleted);
        let already_count = candidates.iter().filter(|c| c.already_filleted).count();
        assert_eq!(
            already_count, 3,
            "All 3 triangle corners should be detected as already filleted, got {already_count}"
        );
    }

    #[test]
    fn apply_radius_preserves_existing_curves() {
        // Path with a CubicTo followed by LineTo corners
        let path = VecPath::parse_svg_d("M0 0 C5 10 15 10 20 0 L20 20 L0 20 Z");
        let result = apply_radius(&path, 2.0);
        // Should still have at least the original cubic plus fillet cubics
        let cubic_count = result.subpaths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, PathCommand::CubicTo { .. }))
            .count();
        assert!(
            cubic_count >= 1,
            "Should preserve original curves, got {} cubics",
            cubic_count
        );
    }

    #[test]
    fn set_start_point_rotates_path() {
        let rect = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        // Set start point near (10, 0) corner
        let result = set_start_point(&rect, 10.0, 0.0);

        assert!(!result.is_empty());
        assert!(result.subpaths[0].closed);
    }

    #[test]
    fn set_start_point_open_path() {
        let path = VecPath::parse_svg_d("M0 0 L10 0 L10 10");
        let result = set_start_point(&path, 10.0, 10.0);

        // Open paths should not be rotated
        assert!(!result.is_empty());
    }

    // ── Close & Join tests ──

    #[test]
    fn close_and_join_chains_matching_endpoints() {
        let a = VecPath::parse_svg_d("M0 0 L10 0");
        let b = VecPath::parse_svg_d("M10 0 L10 10");

        let result = close_and_join(&[a, b], 0.1);
        // Should chain into one SubPath (may or may not be closed depending on start≠end)
        assert_eq!(result.path.subpaths.len(), 1);
    }

    #[test]
    fn close_and_join_reverses_when_needed() {
        let a = VecPath::parse_svg_d("M0 0 L10 0");
        // B's end matches A's end — requires reversal
        let b = VecPath::parse_svg_d("M10 10 L10 0");

        let result = close_and_join(&[a, b], 0.1);
        assert_eq!(result.path.subpaths.len(), 1);
    }

    #[test]
    fn close_and_join_closes_loop() {
        // Three open paths forming a triangle with small gaps
        let a = VecPath::parse_svg_d("M0 0 L10 0");
        let b = VecPath::parse_svg_d("M10 0 L5 8.66");
        let c = VecPath::parse_svg_d("M5 8.66 L0 0");

        let result = close_and_join(&[a, b, c], 0.1);
        assert!(
            result.fully_closed,
            "Triangle chain should close: {:?}",
            result
                .path
                .subpaths
                .iter()
                .map(|sp| sp.closed)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn close_and_join_preserves_closed_subpaths() {
        let closed_sp = VecPath::parse_svg_d("M50 0 L60 0 L60 10 Z");
        let open_a = VecPath::parse_svg_d("M0 0 L10 0");
        let open_b = VecPath::parse_svg_d("M10 0 L10 10 L0 10 L0 0");

        let result = close_and_join(&[closed_sp, open_a, open_b], 0.1);
        assert!(
            result.path.subpaths.len() >= 2,
            "Should have original closed + chained, got {}",
            result.path.subpaths.len()
        );
    }

    #[test]
    fn close_and_join_preserves_curves() {
        let a = VecPath::parse_svg_d("M0 0 C5 10 10 10 15 0");
        let b = VecPath::parse_svg_d("M15 0 L15 10 L0 10 L0 0");

        let result = close_and_join(&[a, b], 0.1);
        let has_cubic = result.path.subpaths.iter().any(|sp| {
            sp.commands
                .iter()
                .any(|c| matches!(c, PathCommand::CubicTo { .. }))
        });
        assert!(has_cubic, "Should preserve cubic curves");
    }

    #[test]
    fn close_and_join_multiple_separate_chains() {
        // Two triangles that form separate loops
        let a1 = VecPath::parse_svg_d("M0 0 L5 0");
        let a2 = VecPath::parse_svg_d("M5 0 L2.5 4 L0 0");
        let b1 = VecPath::parse_svg_d("M20 0 L25 0");
        let b2 = VecPath::parse_svg_d("M25 0 L22.5 4 L20 0");

        let result = close_and_join(&[a1, a2, b1, b2], 0.1);
        let closed_count = result.path.subpaths.iter().filter(|sp| sp.closed).count();
        assert!(
            closed_count >= 2,
            "Should have 2 separate closed chains, got {} closed out of {} total",
            closed_count,
            result.path.subpaths.len()
        );
    }

    #[test]
    fn close_and_join_open_chain_reports_not_fully_closed() {
        let a = VecPath::parse_svg_d("M0 0 L10 0");
        let b = VecPath::parse_svg_d("M10 0 L20 10");
        // Gap between end(20,10) and start(0,0) is >0.1 tolerance

        let result = close_and_join(&[a, b], 0.1);
        assert!(
            !result.fully_closed,
            "Open chain should not be fully_closed"
        );
    }

    #[test]
    fn close_and_join_all_closed_reports_fully_closed() {
        let a = VecPath::parse_svg_d("M0 0 L10 0");
        let b = VecPath::parse_svg_d("M10 0 L10 10 L0 10 L0 0");

        let result = close_and_join(&[a, b], 0.1);
        assert!(result.fully_closed, "Complete loop should be fully_closed");
    }

    #[test]
    fn break_apart_extended_handles_visual_bounds_consistent() {
        // Multi-subpath path where first subpath has extended bezier handles
        let path = "M0 0 C0 500 100 500 100 0 Z M200 200 L300 200 L300 300 Z";
        let parts = break_apart(path);
        assert_eq!(parts.len(), 2);
        // Each part: visual_bounds should be tighter than control hull for cubic
        let p1 = VecPath::parse_svg_d(&parts[0]);
        let vb1 = p1.visual_bounds().unwrap();
        // Curve peaks around y≈375 (not 500 from control points)
        assert!(
            vb1.max.y < 400.0,
            "visual bounds should be tighter than control hull, got max.y={}",
            vb1.max.y
        );
        assert!(
            vb1.max.y > 300.0,
            "curve does bulge significantly, got max.y={}",
            vb1.max.y
        );
    }

    #[test]
    fn break_apart_circle_into_quarter_arcs() {
        // Circle: 4 cubic arcs, last one ends at the start point → Close is NOT emitted
        let circle = "M50 0 C50 27.6 27.6 50 0 50 C-27.6 50 -50 27.6 -50 0 C-50 -27.6 -27.6 -50 0 -50 C27.6 -50 50 -27.6 50 0 Z";
        let parts = break_apart(circle);
        assert_eq!(
            parts.len(),
            4,
            "Circle should break into 4 quarter arcs, got {}",
            parts.len()
        );
        // Each part should be an open cubic (M + C)
        for (i, part) in parts.iter().enumerate() {
            let p = VecPath::parse_svg_d(part);
            assert_eq!(p.subpaths.len(), 1, "Part {i} should have 1 subpath");
            assert!(!p.subpaths[0].closed, "Part {i} should be open");
            assert_eq!(
                p.subpaths[0].commands.len(),
                2,
                "Part {i} should have M + C"
            );
            assert!(
                matches!(p.subpaths[0].commands[1], PathCommand::CubicTo { .. }),
                "Part {i} second command should be CubicTo"
            );
        }
    }

    #[test]
    fn break_apart_rect_into_four_edges() {
        // Rectangle with Close: 3 LineTo + closing edge from (0,50)→(0,0) = 4 parts
        let rect = "M0 0 L100 0 L100 50 L0 50 Z";
        let parts = break_apart(rect);
        assert_eq!(
            parts.len(),
            4,
            "Rect should break into 4 edges (3 LineTo + 1 closing), got {}",
            parts.len()
        );
        // Last part should be the closing edge: M0,50 L0,0
        let last = VecPath::parse_svg_d(parts.last().unwrap());
        let cmds = &last.subpaths[0].commands;
        assert_eq!(cmds.len(), 2);
        match cmds[0] {
            PathCommand::MoveTo { x, y } => {
                assert!(
                    (x - 0.0).abs() < 0.01 && (y - 50.0).abs() < 0.01,
                    "Close edge start should be (0,50)"
                );
            }
            _ => panic!("Expected MoveTo"),
        }
        match cmds[1] {
            PathCommand::LineTo { x, y } => {
                assert!(
                    (x - 0.0).abs() < 0.01 && (y - 0.0).abs() < 0.01,
                    "Close edge end should be (0,0)"
                );
            }
            _ => panic!("Expected LineTo"),
        }
    }

    #[test]
    fn break_apart_triangle_into_three_edges() {
        let triangle = "M0 0 L50 80 L100 0 Z";
        let parts = break_apart(triangle);
        assert_eq!(
            parts.len(),
            3,
            "Triangle should break into 3 edges (2 LineTo + 1 closing), got {}",
            parts.len()
        );
    }

    #[test]
    fn break_apart_single_segment_noop() {
        let line = "M0 0 L10 10";
        let parts = break_apart(line);
        assert_eq!(parts.len(), 1, "Single segment should return as-is");
        assert_eq!(parts[0], VecPath::parse_svg_d(line).to_svg_d());
    }

    #[test]
    fn break_apart_open_polyline_splits_segments() {
        // Open polyline with 2 segments → 2 parts
        let polyline = "M0 0 L10 0 L10 10";
        let parts = break_apart(polyline);
        assert_eq!(
            parts.len(),
            2,
            "Open polyline with 2 segments should produce 2 parts, got {}",
            parts.len()
        );
        // First: M0,0 L10,0
        let p0 = VecPath::parse_svg_d(&parts[0]);
        assert_eq!(p0.subpaths[0].commands.len(), 2);
        // Second: M10,0 L10,10
        let p1 = VecPath::parse_svg_d(&parts[1]);
        assert_eq!(p1.subpaths[0].commands.len(), 2);
    }

    #[test]
    fn close_and_join_result_visual_bounds_consistent() {
        // Join two open paths with curves
        let a = VecPath::parse_svg_d("M0 0 C0 100 50 100 50 0");
        let b = VecPath::parse_svg_d("M50 0 C50 100 100 100 100 0");
        let result = close_and_join(&[a, b], 1.0);
        let vis = result.path.visual_bounds();
        let hull = result.path.bounds();
        if let (Some(v), Some(h)) = (vis, hull) {
            assert!(
                v.max.y <= h.max.y + 1e-6,
                "visual max.y={} should be <= hull max.y={}",
                v.max.y,
                h.max.y
            );
            assert!(
                v.max.y < 80.0,
                "curve peaks, not at control point y=100, got max.y={}",
                v.max.y
            );
        }
    }

    // ── Normalization / denormalization tests ──

    #[test]
    fn normalize_adds_closing_lineto() {
        // M0,0 L10,0 L10,10 Z  — Close edge goes 10,10→0,0 (non-degenerate)
        let sp = SubPath {
            commands: vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 10.0 },
                PathCommand::Close,
            ],
            closed: true,
        };
        let (norm, was_modified) = normalize_closed_subpath(&sp);
        assert!(was_modified);
        // Should have M, L, L, L(0,0), Z = 5 commands
        assert_eq!(norm.commands.len(), 5);
        assert!(
            matches!(norm.commands[3], PathCommand::LineTo { x, y } if x.abs() < 1e-9 && y.abs() < 1e-9)
        );
    }

    #[test]
    fn normalize_noop_when_already_degenerate() {
        // M0,0 L10,0 L10,10 L0,0 Z — last LineTo already ends at MoveTo
        let sp = SubPath {
            commands: vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 10.0 },
                PathCommand::LineTo { x: 0.0, y: 0.0 },
                PathCommand::Close,
            ],
            closed: true,
        };
        let (norm, was_modified) = normalize_closed_subpath(&sp);
        assert!(!was_modified);
        assert_eq!(norm.commands.len(), 5);
    }

    #[test]
    fn denormalize_removes_closing_lineto() {
        let sp = SubPath {
            commands: vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 10.0 },
                PathCommand::LineTo { x: 0.0, y: 0.0 },
                PathCommand::Close,
            ],
            closed: true,
        };
        let denorm = denormalize_closed_subpath(&sp);
        // Should have M, L, L, Z = 4 commands
        assert_eq!(denorm.commands.len(), 4);
        assert!(
            matches!(denorm.commands[2], PathCommand::LineTo { x, y } if (x - 10.0).abs() < 1e-9 && (y - 10.0).abs() < 1e-9)
        );
        assert!(matches!(denorm.commands[3], PathCommand::Close));
    }

    #[test]
    fn normalize_then_denormalize_roundtrips() {
        let sp = SubPath {
            commands: vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 10.0 },
                PathCommand::Close,
            ],
            closed: true,
        };
        let (norm, _) = normalize_closed_subpath(&sp);
        let denorm = denormalize_closed_subpath(&norm);
        assert_eq!(denorm.commands.len(), sp.commands.len());
    }

    // ── Rotation tests ──

    #[test]
    fn rotate_subpath_preserves_curves() {
        // M(A) C(c1,c2,B) L(C) L(A) Z — normalized, 3 display vertices
        let path = VecPath::parse_svg_d("M0 0 C5 10 15 10 20 0 L20 20 L0 0 Z");
        let rotated = rotate_subpath_start(&path, 0, 1);
        let has_cubic = rotated.subpaths[0]
            .commands
            .iter()
            .any(|c| matches!(c, PathCommand::CubicTo { .. }));
        assert!(has_cubic, "Rotation should preserve CubicTo");
        assert!(rotated.subpaths[0].closed);
    }

    #[test]
    fn rotate_subpath_noop_at_zero() {
        let path = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 0 Z");
        let rotated = rotate_subpath_start(&path, 0, 0);
        assert_eq!(rotated.to_svg_d(), path.to_svg_d());
    }

    #[test]
    fn rotate_subpath_start_to_vertex2() {
        // Square: M(0,0) L(10,0) L(10,10) L(0,0) Z — normalized, v_display=3
        let path = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 0 Z");
        let rotated = rotate_subpath_start(&path, 0, 2);
        // New start should be (10,10)
        match rotated.subpaths[0].commands[0] {
            PathCommand::MoveTo { x, y } => {
                assert!((x - 10.0).abs() < 1e-9, "Expected x=10, got {x}");
                assert!((y - 10.0).abs() < 1e-9, "Expected y=10, got {y}");
            }
            _ => panic!("Expected MoveTo"),
        }
    }

    // ── Reverse tests ──

    #[test]
    fn reverse_subpath_at_reverses_direction() {
        let path = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 0 Z");
        let reversed = reverse_subpath_at(&path, 0);
        // First command should still be MoveTo but the drawing direction is reversed
        assert!(matches!(
            reversed.subpaths[0].commands[0],
            PathCommand::MoveTo { .. }
        ));
        assert!(reversed.subpaths[0].closed);
    }

    // ── get_path_vertices tests ──

    #[test]
    fn get_path_vertices_rect() {
        let path = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let verts = get_path_vertices(&path);
        // 4 commands (M, L, L, L) + Close, no normalized closing = 4 display vertices
        assert_eq!(
            verts.len(),
            4,
            "Rectangle should have 4 display vertices, got {}",
            verts.len()
        );
        assert!(verts[0].is_start);
        assert!(verts[0].subpath_closed);
    }

    #[test]
    fn get_path_vertices_suppresses_normalized_closing() {
        // Normalized: M(0,0) L(10,0) L(10,10) L(0,0) Z — last LineTo duplicates MoveTo
        let path = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 0 Z");
        let verts = get_path_vertices(&path);
        assert_eq!(
            verts.len(),
            3,
            "Should suppress normalized closing vertex, got {}",
            verts.len()
        );
    }

    #[test]
    fn get_path_vertices_open_path() {
        let path = VecPath::parse_svg_d("M0 0 L10 0 L10 10");
        let verts = get_path_vertices(&path);
        assert_eq!(verts.len(), 3);
        assert!(!verts[0].subpath_closed);
    }

    // ── Full start-point workflow trace ──

    #[test]
    fn start_point_full_workflow_trace() {
        use crate::object::StartPointEdit;

        // Original: M(A=0,0) L(B=10,0) L(C=10,10) L(D=0,10) Z
        let original = "M0 0 L10 0 L10 10 L0 10 Z";
        let mut vp = VecPath::parse_svg_d(original);

        // Step 1: Normalize
        let (norm_sp, was_modified) = normalize_closed_subpath(&vp.subpaths[0]);
        assert!(was_modified);
        vp.subpaths[0] = norm_sp;
        let v_display = vp.subpaths[0]
            .commands
            .iter()
            .filter(|c| !matches!(c, PathCommand::Close))
            .count()
            - 1;
        assert_eq!(v_display, 4, "v_display should be 4");

        let mut entry = StartPointEdit {
            subpath_index: 0,
            original_start_current_idx: 0,
            reversed: false,
            v_display,
            normalized: true,
        };

        // Step 2: Rotate to vertex C (idx=2)
        vp = rotate_subpath_start(&vp, 0, 2);
        entry.original_start_current_idx =
            (entry.original_start_current_idx + v_display - 2) % v_display;
        assert_eq!(entry.original_start_current_idx, 2);

        // Verify new start is C(10,10)
        match vp.subpaths[0].commands[0] {
            PathCommand::MoveTo { x, y } => {
                assert!(
                    (x - 10.0).abs() < 1e-6 && (y - 10.0).abs() < 1e-6,
                    "Expected start at C(10,10), got ({x},{y})"
                );
            }
            _ => panic!("Expected MoveTo"),
        }

        // Step 3: Reset — rotate back to original_start_current_idx
        vp = rotate_subpath_start(&vp, 0, entry.original_start_current_idx);
        // Denormalize
        vp.subpaths[0] = denormalize_closed_subpath(&vp.subpaths[0]);

        // Verify we're back at A(0,0)
        match vp.subpaths[0].commands[0] {
            PathCommand::MoveTo { x, y } => {
                assert!(
                    (x).abs() < 1e-6 && (y).abs() < 1e-6,
                    "Expected reset to A(0,0), got ({x},{y})"
                );
            }
            _ => panic!("Expected MoveTo"),
        }
        // Verify command count matches original
        let orig_vp = VecPath::parse_svg_d(original);
        assert_eq!(
            vp.subpaths[0].commands.len(),
            orig_vp.subpaths[0].commands.len(),
            "Command count should match original after reset"
        );
    }

    // ── Seam fillet bug fix + per-corner radius tests ──

    #[test]
    fn apply_radius_no_double_seam_fillet() {
        // Rectangle: 4 corners, all LineTo→LineTo → exactly 4 CubicTo arcs
        let rect = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let result = apply_radius(&rect, 2.0);
        let cubic_count = result.subpaths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, PathCommand::CubicTo { .. }))
            .count();
        assert_eq!(
            cubic_count, 4,
            "Rectangle fillet should produce exactly 4 CubicTo arcs, got {cubic_count}"
        );
    }

    #[test]
    fn get_fillet_candidates_rectangle() {
        let rect = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let candidates = get_fillet_candidates(&rect);
        assert_eq!(
            candidates.len(),
            4,
            "Rectangle should have 4 fillet candidates, got {}",
            candidates.len()
        );
        assert!(
            candidates.iter().all(|c| !c.already_filleted),
            "Fresh rectangle corners should not be already_filleted"
        );
    }

    #[test]
    fn get_fillet_candidates_mixed() {
        // CubicTo followed by LineTo corners — only LineTo→LineTo qualifies
        let path = VecPath::parse_svg_d("M0 0 C5 10 15 10 20 0 L20 20 L0 20 Z");
        let candidates = get_fillet_candidates(&path);
        // Corner at (20,0): incoming is CubicTo → disqualified
        // Corner at (20,20): LineTo→LineTo → qualifies
        // Corner at (0,20): LineTo→LineTo(closing edge)→qualifies
        // Corner at (0,0)=move_pt: closing edge(LineTo) → first cmd(CubicTo) → disqualified
        assert_eq!(
            candidates.len(),
            2,
            "Mixed path should have 2 fillet candidates, got {}",
            candidates.len()
        );
    }

    #[test]
    fn apply_radius_at_corner_single() {
        let rect = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let candidates = get_fillet_candidates(&rect);
        assert_eq!(candidates.len(), 4);
        // Fillet only the first candidate
        let result = apply_radius_at_corner(
            &rect,
            candidates[0].subpath_index,
            candidates[0].vertex_index,
            2.0,
        );
        let cubic_count = result.subpaths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, PathCommand::CubicTo { .. }))
            .count();
        assert_eq!(
            cubic_count, 1,
            "Single corner fillet should produce 1 CubicTo, got {cubic_count}"
        );
    }

    #[test]
    fn apply_radius_at_corner_closing() {
        // Fillet the closing corner (vertex 0 = last candidate for a rectangle)
        let rect = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let candidates = get_fillet_candidates(&rect);
        let last = candidates.last().unwrap();
        let result = apply_radius_at_corner(&rect, last.subpath_index, last.vertex_index, 2.0);
        let cubic_count = result.subpaths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, PathCommand::CubicTo { .. }))
            .count();
        assert_eq!(
            cubic_count, 1,
            "Closing corner fillet should produce 1 CubicTo, got {cubic_count}"
        );
        // MoveTo should be adjusted (not at original 0,0)
        match result.subpaths[0].commands[0] {
            PathCommand::MoveTo { x, y } => {
                let moved = (x - 0.0).abs() > 0.01 || (y - 0.0).abs() > 0.01;
                assert!(
                    moved,
                    "MoveTo should be adjusted for closing corner fillet, got ({x},{y})"
                );
            }
            _ => panic!("Expected MoveTo"),
        }
    }

    #[test]
    fn apply_radius_at_corner_negative() {
        // 20×20 rectangle, negative radius on first corner (at (20,0)).
        // Dog-bone model: arc centered at corner vertex, short sweep through interior.
        let rect = VecPath::parse_svg_d("M0 0 L20 0 L20 20 L0 20 Z");
        let candidates = get_fillet_candidates(&rect);
        let result = apply_radius_at_corner(
            &rect,
            candidates[0].subpath_index,
            candidates[0].vertex_index,
            -3.0,
        );
        let cubics: Vec<_> = result.subpaths[0]
            .commands
            .iter()
            .filter_map(|c| match c {
                PathCommand::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                } => Some((*c1x, *c1y, *c2x, *c2y, *x, *y)),
                _ => None,
            })
            .collect();
        // 90° corner: dog-bone sweep = π/2 → 1 cubic segment (not 3)
        assert_eq!(
            cubics.len(),
            1,
            "Dog-bone on 90° corner should produce 1 CubicTo, got {}",
            cubics.len()
        );

        // The arc should bow AWAY from the corner (toward the interior), creating
        // a concave notch. Evaluate at t=0.5 and check that the midpoint is on the
        // INTERIOR side of the chord (further from corner than chord midpoint).
        // tangent_in = (17, 0), tangent_out = (20, 3), corner = (20, 0).
        let (c1x, c1y, c2x, c2y, ex, ey) = cubics[0];
        // tangent_in is the endpoint of the shortened LineTo before this cubic.
        // For the first fillet in a rectangle, tangent_in = (17, 0).
        let ti_x = 17.0;
        let ti_y = 0.0;
        let mid_x = 0.125 * ti_x + 0.375 * c1x + 0.375 * c2x + 0.125 * ex;
        let mid_y = 0.125 * ti_y + 0.375 * c1y + 0.375 * c2y + 0.125 * ey;

        let corner_x = 20.0_f64;
        let corner_y = 0.0_f64;
        let chord_mid_x = (17.0 + 20.0) / 2.0;
        let chord_mid_y = (0.0 + 3.0) / 2.0;
        let dist_chord =
            ((chord_mid_x - corner_x).powi(2) + (chord_mid_y - corner_y).powi(2)).sqrt();
        let dist_arc = ((mid_x - corner_x).powi(2) + (mid_y - corner_y).powi(2)).sqrt();
        assert!(
            dist_arc > dist_chord,
            "Arc midpoint ({mid_x:.2}, {mid_y:.2}) dist {dist_arc:.2} should be further from corner than chord midpoint dist {dist_chord:.2}"
        );

        // Endpoint should be tangent_out on the outgoing edge
        assert!((ex - 20.0).abs() < 0.01, "Endpoint x={ex} should be ≈20");
        assert!(
            (ey - 3.0).abs() < 0.01,
            "Endpoint y={ey} should be ≈3 (tangent_out on outgoing edge)"
        );
    }

    #[test]
    fn get_fillet_candidates_detects_already_filleted() {
        // Fillet one corner of a rectangle, then verify candidates include it as already_filleted
        let rect = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let candidates = get_fillet_candidates(&rect);
        assert_eq!(candidates.len(), 4);
        assert!(candidates.iter().all(|c| !c.already_filleted));

        // Fillet corner 0 (at (10,0))
        let filleted = apply_radius_at_corner(
            &rect,
            candidates[0].subpath_index,
            candidates[0].vertex_index,
            2.0,
        );
        let candidates2 = get_fillet_candidates(&filleted);

        // Should still have 4 candidates: 3 unfilleted + 1 already_filleted
        assert_eq!(
            candidates2.len(),
            4,
            "Expected 4 candidates after filleting one corner, got {}",
            candidates2.len()
        );
        let filleted_count = candidates2.iter().filter(|c| c.already_filleted).count();
        assert_eq!(
            filleted_count, 1,
            "Expected 1 already_filleted candidate, got {filleted_count}"
        );
    }

    #[test]
    fn unfillet_restores_original_corner() {
        // Fillet one corner then unfillet it — should restore the original rectangle
        let rect = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let candidates = get_fillet_candidates(&rect);
        let filleted = apply_radius_at_corner(
            &rect,
            candidates[0].subpath_index,
            candidates[0].vertex_index,
            2.0,
        );

        // Verify it has a CubicTo
        let cubic_count = filleted.subpaths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, PathCommand::CubicTo { .. }))
            .count();
        assert_eq!(cubic_count, 1);

        // Now unfillet by applying radius 0 to the already-filleted corner
        let candidates2 = get_fillet_candidates(&filleted);
        let filleted_candidate = candidates2.iter().find(|c| c.already_filleted).unwrap();
        let restored = apply_radius_at_corner(
            &filleted,
            filleted_candidate.subpath_index,
            filleted_candidate.vertex_index,
            0.0,
        );

        // Should have no CubicTo
        let cubic_count2 = restored.subpaths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, PathCommand::CubicTo { .. }))
            .count();
        assert_eq!(
            cubic_count2, 0,
            "Unfilleting should remove CubicTo, got {cubic_count2}"
        );

        // Should have original corner at (10,0)
        let candidates3 = get_fillet_candidates(&restored);
        assert_eq!(candidates3.len(), 4);
        assert!(candidates3.iter().all(|c| !c.already_filleted));
    }

    #[test]
    fn refillet_already_filleted_corner() {
        // Fillet a corner, then re-fillet it with a different radius
        let rect = VecPath::parse_svg_d("M0 0 L20 0 L20 20 L0 20 Z");
        let candidates = get_fillet_candidates(&rect);
        let original_corner = (candidates[0].x, candidates[0].y);
        let filleted1 = apply_radius_at_corner(
            &rect,
            candidates[0].subpath_index,
            candidates[0].vertex_index,
            3.0,
        );

        // Re-fillet the already-filleted corner with a different radius
        let candidates2 = get_fillet_candidates(&filleted1);
        let filleted_candidate = candidates2.iter().find(|c| c.already_filleted).unwrap();
        let filleted2 = apply_radius_at_corner(
            &filleted1,
            filleted_candidate.subpath_index,
            filleted_candidate.vertex_index,
            5.0,
        );

        // Should still have exactly 1 CubicTo (unfillet + re-fillet)
        let cubic_count = filleted2.subpaths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, PathCommand::CubicTo { .. }))
            .count();
        assert_eq!(
            cubic_count, 1,
            "Re-fillet should produce 1 CubicTo, got {cubic_count}"
        );

        // The filleted candidate should be at the SAME corner position as the original
        let candidates3 = get_fillet_candidates(&filleted2);
        let refilleted = candidates3.iter().find(|c| c.already_filleted).unwrap();
        assert!(
            (refilleted.x - original_corner.0).abs() < 0.1
                && (refilleted.y - original_corner.1).abs() < 0.1,
            "Re-filleted corner should be at ({}, {}), got ({}, {})",
            original_corner.0,
            original_corner.1,
            refilleted.x,
            refilleted.y,
        );
    }

    #[test]
    fn refillet_hexagon_preserves_corner_identity() {
        // Regular hexagon — fillet corner 0, then re-fillet with a different radius.
        // Verify only corner 0 is filleted, no adjacent corners changed.
        use std::f64::consts::PI;
        let r = 20.0;
        let mut cmds = Vec::new();
        for i in 0..6 {
            let angle = PI / 3.0 * i as f64;
            let x = r * angle.cos();
            let y = r * angle.sin();
            if i == 0 {
                cmds.push(PathCommand::MoveTo { x, y });
            } else {
                cmds.push(PathCommand::LineTo { x, y });
            }
        }
        cmds.push(PathCommand::Close);
        let hex = VecPath {
            subpaths: vec![SubPath {
                commands: cmds,
                closed: true,
            }],
        };

        let candidates = get_fillet_candidates(&hex);
        assert_eq!(
            candidates.len(),
            6,
            "Hexagon should have 6 fillet candidates"
        );
        let corner0_pos = (candidates[0].x, candidates[0].y);

        // Fillet corner 0 with radius 3
        let filleted1 = apply_radius_at_corner(
            &hex,
            candidates[0].subpath_index,
            candidates[0].vertex_index,
            3.0,
        );

        // Re-fillet corner 0 with radius 5
        let candidates2 = get_fillet_candidates(&filleted1);
        let filleted_c = candidates2.iter().find(|c| c.already_filleted).unwrap();
        assert!(
            (filleted_c.x - corner0_pos.0).abs() < 0.1
                && (filleted_c.y - corner0_pos.1).abs() < 0.1,
            "Filleted candidate should be at corner 0"
        );
        let filleted2 = apply_radius_at_corner(
            &filleted1,
            filleted_c.subpath_index,
            filleted_c.vertex_index,
            5.0,
        );

        // Verify: exactly 1 CubicTo, at corner 0's position
        let candidates3 = get_fillet_candidates(&filleted2);
        let filleted_corners: Vec<_> = candidates3.iter().filter(|c| c.already_filleted).collect();
        assert_eq!(
            filleted_corners.len(),
            1,
            "Only 1 corner should be filleted, got {}",
            filleted_corners.len()
        );
        assert!(
            (filleted_corners[0].x - corner0_pos.0).abs() < 0.1
                && (filleted_corners[0].y - corner0_pos.1).abs() < 0.1,
            "The filleted corner should be at corner 0 ({}, {}), got ({}, {})",
            corner0_pos.0,
            corner0_pos.1,
            filleted_corners[0].x,
            filleted_corners[0].y,
        );
        let unfilleted_corners: Vec<_> =
            candidates3.iter().filter(|c| !c.already_filleted).collect();
        assert_eq!(
            unfilleted_corners.len(),
            5,
            "5 corners should remain unfilleted, got {}",
            unfilleted_corners.len()
        );
    }

    #[test]
    fn refillet_closing_corner_preserves_identity() {
        // Fillet the closing corner of a rectangle, then re-fillet with a different radius.
        let rect = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let candidates = get_fillet_candidates(&rect);
        // The closing corner is the last candidate (at origin, where last edge meets first)
        let closing = candidates.last().unwrap();
        let closing_pos = (closing.x, closing.y);

        // Fillet closing corner with radius 2
        let filleted1 =
            apply_radius_at_corner(&rect, closing.subpath_index, closing.vertex_index, 2.0);
        let candidates2 = get_fillet_candidates(&filleted1);
        let filleted_c = candidates2.iter().find(|c| c.already_filleted).unwrap();

        // Re-fillet closing corner with radius 3
        let filleted2 = apply_radius_at_corner(
            &filleted1,
            filleted_c.subpath_index,
            filleted_c.vertex_index,
            3.0,
        );

        // Verify: exactly 1 CubicTo, at the closing corner position
        let candidates3 = get_fillet_candidates(&filleted2);
        let filleted_corners: Vec<_> = candidates3.iter().filter(|c| c.already_filleted).collect();
        assert_eq!(
            filleted_corners.len(),
            1,
            "Only 1 corner should be filleted, got {}",
            filleted_corners.len()
        );
        assert!(
            (filleted_corners[0].x - closing_pos.0).abs() < 0.1
                && (filleted_corners[0].y - closing_pos.1).abs() < 0.1,
            "Filleted corner should be at closing position ({}, {}), got ({}, {})",
            closing_pos.0,
            closing_pos.1,
            filleted_corners[0].x,
            filleted_corners[0].y,
        );
    }

    #[test]
    fn unfillet_closing_corner_restores_moveto() {
        // Fillet the closing corner, then unfillet — MoveTo should be restored
        let rect = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let candidates = get_fillet_candidates(&rect);
        let last = candidates.last().unwrap();
        let filleted = apply_radius_at_corner(&rect, last.subpath_index, last.vertex_index, 2.0);

        // MoveTo should have been adjusted away from (0,0)
        match filleted.subpaths[0].commands[0] {
            PathCommand::MoveTo { x, y } => {
                assert!(
                    (x - 0.0).abs() > 0.01 || (y - 0.0).abs() > 0.01,
                    "MoveTo should be adjusted after filleting closing corner"
                );
            }
            _ => panic!("Expected MoveTo"),
        }

        // Now unfillet
        let candidates2 = get_fillet_candidates(&filleted);
        let filleted_candidate = candidates2.iter().find(|c| c.already_filleted).unwrap();
        let restored = apply_radius_at_corner(
            &filleted,
            filleted_candidate.subpath_index,
            filleted_candidate.vertex_index,
            0.0,
        );

        // MoveTo should be restored to (0,0)
        match restored.subpaths[0].commands[0] {
            PathCommand::MoveTo { x, y } => {
                assert!(
                    (x).abs() < 0.1 && (y).abs() < 0.1,
                    "MoveTo should be restored to ~(0,0) after unfilleting closing corner, got ({x},{y})"
                );
            }
            _ => panic!("Expected MoveTo"),
        }
    }

    #[test]
    fn deliberate_curve_between_lines_not_flagged_as_fillet() {
        // A path with a deliberate artistic CubicTo between two LineTos.
        // The control points are NOT tangent to the adjacent lines, so this
        // must NOT be classified as an already-filleted corner.
        // Shape: line to (10,0), then a curve bulging up to (15,10) via
        // control points that are clearly off the line directions, then line to (20,0).
        let path = VecPath::parse_svg_d("M0 0 L10 0 C12 8 18 8 20 0 L30 0");
        let candidates = get_fillet_candidates(&path);
        // The CubicTo control points (12,8) and (18,8) are NOT collinear with
        // the incoming line (horizontal) or outgoing line (horizontal),
        // so no already_filleted candidate should appear.
        for c in &candidates {
            assert!(
                !c.already_filleted,
                "Deliberate curve should not be flagged as already_filleted, got candidate at ({}, {})",
                c.x, c.y
            );
        }
    }

    #[test]
    fn fillet_then_detect_is_consistent() {
        // Fillet a corner, then verify the fillet IS detected as already_filleted.
        // This confirms the geometric check accepts actual fillets.
        let rect = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let candidates = get_fillet_candidates(&rect);
        let filleted = apply_radius_at_corner(
            &rect,
            candidates[0].subpath_index,
            candidates[0].vertex_index,
            2.0,
        );
        let candidates2 = get_fillet_candidates(&filleted);
        let filleted_count = candidates2.iter().filter(|c| c.already_filleted).count();
        assert_eq!(
            filleted_count, 1,
            "A filleted corner must be detected as already_filleted, got {filleted_count}"
        );

        // Also fillet a second corner to make sure both are detected
        let sharp = candidates2.iter().find(|c| !c.already_filleted).unwrap();
        let filleted2 =
            apply_radius_at_corner(&filleted, sharp.subpath_index, sharp.vertex_index, 3.0);
        let candidates3 = get_fillet_candidates(&filleted2);
        let filleted_count2 = candidates3.iter().filter(|c| c.already_filleted).count();
        assert_eq!(
            filleted_count2, 2,
            "Two filleted corners should be detected, got {filleted_count2}"
        );
    }

    #[test]
    fn negative_fillet_detected_as_already_filleted() {
        // Apply negative (dog-bone) radius, then verify it's detected as already_filleted.
        let rect = VecPath::parse_svg_d("M0 0 L20 0 L20 20 L0 20 Z");
        let candidates = get_fillet_candidates(&rect);
        let filleted = apply_radius_at_corner(
            &rect,
            candidates[0].subpath_index,
            candidates[0].vertex_index,
            -3.0,
        );
        let candidates2 = get_fillet_candidates(&filleted);
        let filleted_count = candidates2.iter().filter(|c| c.already_filleted).count();
        assert_eq!(
            filleted_count, 1,
            "Dog-bone fillet must be detected as already_filleted, got {filleted_count}"
        );
    }

    #[test]
    fn negative_fillet_unfillet_and_refillet() {
        // Apply negative radius, unfillet (radius=0), verify sharp corner restored,
        // then re-fillet with positive radius.
        let rect = VecPath::parse_svg_d("M0 0 L20 0 L20 20 L0 20 Z");
        let candidates = get_fillet_candidates(&rect);
        let filleted = apply_radius_at_corner(
            &rect,
            candidates[0].subpath_index,
            candidates[0].vertex_index,
            -3.0,
        );

        // Unfillet
        let candidates2 = get_fillet_candidates(&filleted);
        let dog = candidates2.iter().find(|c| c.already_filleted).unwrap();
        let restored = apply_radius_at_corner(&filleted, dog.subpath_index, dog.vertex_index, 0.0);
        let cubic_count = restored.subpaths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, PathCommand::CubicTo { .. }))
            .count();
        assert_eq!(
            cubic_count, 0,
            "Unfilleted path should have no CubicTo, got {cubic_count}"
        );

        // Re-fillet with positive radius
        let candidates3 = get_fillet_candidates(&restored);
        let sharp = candidates3
            .iter()
            .find(|c| !c.already_filleted && (c.x - 20.0).abs() < 1.0 && c.y.abs() < 1.0)
            .unwrap();
        let refilleted =
            apply_radius_at_corner(&restored, sharp.subpath_index, sharp.vertex_index, 2.0);
        let cubic_count2 = refilleted.subpaths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, PathCommand::CubicTo { .. }))
            .count();
        assert_eq!(
            cubic_count2, 1,
            "Re-filleted path should have 1 CubicTo, got {cubic_count2}"
        );
    }

    // ── apply_start_point_edits_forward tests ──

    #[test]
    fn forward_edits_rotation_only() {
        use crate::object::StartPointEdit;
        // Rectangle: A(0,0) B(10,0) C(10,10) D(0,10), normalized adds E(0,0) before Close
        let vp = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        // Simulate "set start to vertex 2" (C): osci = (0 + 4 - 2) % 4 = 2
        let edits = vec![StartPointEdit {
            subpath_index: 0,
            original_start_current_idx: 2,
            reversed: false,
            v_display: 4,
            normalized: true,
        }];
        let result = apply_start_point_edits_forward(&vp, &edits);
        // Forward rotation = (4 - 2) % 4 = 2 → start at C(10,10)
        match result.subpaths[0].commands[0] {
            PathCommand::MoveTo { x, y } => {
                assert!(
                    (x - 10.0).abs() < 1e-6 && (y - 10.0).abs() < 1e-6,
                    "Expected start at C(10,10), got ({x},{y})"
                );
            }
            _ => panic!("Expected MoveTo"),
        }
    }

    #[test]
    fn forward_edits_rotation_and_reverse() {
        use crate::object::StartPointEdit;
        let vp = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        // Simulate: set to v2 then reverse
        // After set v2: osci = 2. After reverse: osci = 4 - 2 = 2. reversed = true.
        let edits = vec![StartPointEdit {
            subpath_index: 0,
            original_start_current_idx: 2,
            reversed: false,
            v_display: 4,
            normalized: true,
        }];
        let rotated = apply_start_point_edits_forward(&vp, &edits);
        // Start should be C(10,10) after rotation
        match rotated.subpaths[0].commands[0] {
            PathCommand::MoveTo { x, y } => {
                assert!((x - 10.0).abs() < 1e-6 && (y - 10.0).abs() < 1e-6);
            }
            _ => panic!("Expected MoveTo"),
        }

        // Now with reversed=true, osci stays 2
        let edits_rev = vec![StartPointEdit {
            subpath_index: 0,
            original_start_current_idx: 2,
            reversed: true,
            v_display: 4,
            normalized: true,
        }];
        let rev_result = apply_start_point_edits_forward(&vp, &edits_rev);
        // Start still at C(10,10) (rotation puts C first, reversal keeps start)
        match rev_result.subpaths[0].commands[0] {
            PathCommand::MoveTo { x, y } => {
                assert!(
                    (x - 10.0).abs() < 1e-6 && (y - 10.0).abs() < 1e-6,
                    "Expected start at C(10,10) after reverse, got ({x},{y})"
                );
            }
            _ => panic!("Expected MoveTo"),
        }
        // Verify direction reversed: second vertex should be B(10,0) not D(0,10)
        let second_end = command_endpoint(Some(&rev_result.subpaths[0].commands[1])).unwrap();
        // After rotation to C then reverse: C→B→A→D→C, so second is B(10,0)
        assert!(
            (second_end.x - 10.0).abs() < 1e-6 && second_end.y.abs() < 1e-6,
            "Expected second vertex B(10,0), got ({},{})",
            second_end.x,
            second_end.y
        );
    }

    #[test]
    fn forward_edits_roundtrip_matches_eager() {
        use crate::object::StartPointEdit;
        // Verify lazy forward application matches eager rotation.
        let original = "M0 0 L10 0 L10 10 L0 10 Z";
        let mut vp = VecPath::parse_svg_d(original);

        // Eager: normalize → rotate by 3
        let (norm_sp, _) = normalize_closed_subpath(&vp.subpaths[0]);
        vp.subpaths[0] = norm_sp;
        let eager = rotate_subpath_start(&vp, 0, 3);

        // Lazy: apply_start_point_edits_forward with equivalent entry
        // After rotate by 3: osci = (0 + 4 - 3) % 4 = 1
        let edits = vec![StartPointEdit {
            subpath_index: 0,
            original_start_current_idx: 1,
            reversed: false,
            v_display: 4,
            normalized: true,
        }];
        let lazy = apply_start_point_edits_forward(&VecPath::parse_svg_d(original), &edits);

        // Both should start at D(0,10)
        let eager_start = match eager.subpaths[0].commands[0] {
            PathCommand::MoveTo { x, y } => (x, y),
            _ => panic!("Expected MoveTo"),
        };
        let lazy_start = match lazy.subpaths[0].commands[0] {
            PathCommand::MoveTo { x, y } => (x, y),
            _ => panic!("Expected MoveTo"),
        };
        assert!(
            (eager_start.0 - lazy_start.0).abs() < 1e-6
                && (eager_start.1 - lazy_start.1).abs() < 1e-6,
            "Eager ({},{}) vs lazy ({},{}) mismatch",
            eager_start.0,
            eager_start.1,
            lazy_start.0,
            lazy_start.1
        );
    }
}

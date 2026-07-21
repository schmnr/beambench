use beambench_common::geometry::Point2D;
use beambench_common::path::{PathCommand, SubPath, VecPath};
use serde::{Deserialize, Serialize};

use crate::vector::trim::split_cubic;

/// Identifies a node within a VecPath by subpath index and command index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId {
    pub subpath_idx: usize,
    pub command_idx: usize,
}

/// Which handle of a bezier node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandleType {
    /// Incoming control point (c2 of the previous cubic, or cx of the previous quad).
    In,
    /// Outgoing control point (c1 of the current cubic).
    Out,
}

/// The type of a node (affects handle behavior).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    /// Handles move together symmetrically.
    Smooth,
    /// Handles are independent.
    Corner,
}

/// A node in an editable path, with its position and optional handles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathNode {
    pub id: NodeId,
    pub position: Point2D,
    pub handle_in: Option<Point2D>,
    pub handle_out: Option<Point2D>,
    pub node_type: NodeType,
}

/// An editable representation of a VecPath with indexed nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditablePath {
    pub nodes: Vec<PathNode>,
    pub closed: bool,
}

fn classify_node_type(
    handle_in: Option<Point2D>,
    handle_out: Option<Point2D>,
    node_pos: Point2D,
) -> NodeType {
    let (Some(handle_in), Some(handle_out)) = (handle_in, handle_out) else {
        return NodeType::Corner;
    };
    let in_vec = Point2D::new(handle_in.x - node_pos.x, handle_in.y - node_pos.y);
    let out_vec = Point2D::new(handle_out.x - node_pos.x, handle_out.y - node_pos.y);
    let in_len = (in_vec.x * in_vec.x + in_vec.y * in_vec.y).sqrt();
    let out_len = (out_vec.x * out_vec.x + out_vec.y * out_vec.y).sqrt();
    if in_len < 1e-9 || out_len < 1e-9 {
        return NodeType::Corner;
    }

    let cross = in_vec.x * out_vec.y - in_vec.y * out_vec.x;
    let dot = in_vec.x * out_vec.x + in_vec.y * out_vec.y;
    if dot < 0.0 && cross.abs() <= in_len * out_len * 1e-6 {
        NodeType::Smooth
    } else {
        NodeType::Corner
    }
}

impl EditablePath {
    /// Build an EditablePath from a VecPath.
    pub fn from_vecpath(path: &VecPath) -> Vec<EditablePath> {
        let mut result = Vec::new();

        for (sp_idx, subpath) in path.subpaths.iter().enumerate() {
            let mut nodes = Vec::new();

            for (cmd_idx, cmd) in subpath.commands.iter().enumerate() {
                match *cmd {
                    PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => {
                        let node_pos = Point2D::new(x, y);
                        let handle_in = get_incoming_handle(subpath, cmd_idx, node_pos);
                        let handle_out = get_outgoing_handle(subpath, cmd_idx, node_pos);
                        nodes.push(PathNode {
                            id: NodeId {
                                subpath_idx: sp_idx,
                                command_idx: cmd_idx,
                            },
                            position: Point2D::new(x, y),
                            handle_in,
                            handle_out,
                            node_type: classify_node_type(handle_in, handle_out, node_pos),
                        });
                    }
                    PathCommand::QuadTo { x, y, .. } => {
                        let node_pos = Point2D::new(x, y);
                        let handle_in = get_incoming_handle(subpath, cmd_idx, node_pos);
                        let handle_out = get_outgoing_handle(subpath, cmd_idx, node_pos);
                        nodes.push(PathNode {
                            id: NodeId {
                                subpath_idx: sp_idx,
                                command_idx: cmd_idx,
                            },
                            position: Point2D::new(x, y),
                            handle_in,
                            handle_out,
                            node_type: classify_node_type(handle_in, handle_out, node_pos),
                        });
                    }
                    PathCommand::CubicTo { x, y, c2x, c2y, .. } => {
                        let node_pos = Point2D::new(x, y);
                        let handle_in =
                            filter_zero_length_handle(Some(Point2D::new(c2x, c2y)), node_pos);
                        let handle_out = get_outgoing_handle(subpath, cmd_idx, node_pos);
                        nodes.push(PathNode {
                            id: NodeId {
                                subpath_idx: sp_idx,
                                command_idx: cmd_idx,
                            },
                            position: Point2D::new(x, y),
                            handle_in,
                            handle_out,
                            node_type: classify_node_type(handle_in, handle_out, node_pos),
                        });
                    }
                    PathCommand::Close => {
                        // Close doesn't create a node
                    }
                }
            }

            // Merge coincident close-point: if the subpath is closed and the
            // last node sits at the same position as the first (e.g. ellipse
            // whose last CubicTo endpoint == MoveTo start), remove the
            // duplicate and transfer its handle_in to node 0.
            if subpath.closed && nodes.len() >= 2 {
                let first_pos = nodes[0].position;
                let last_pos = nodes.last().unwrap().position;
                if (first_pos.x - last_pos.x).abs() < 1e-9
                    && (first_pos.y - last_pos.y).abs() < 1e-9
                {
                    let last_node = nodes.pop().unwrap();
                    if last_node.handle_in.is_some() {
                        nodes[0].handle_in = last_node.handle_in;
                    }
                    nodes[0].node_type = classify_node_type(
                        nodes[0].handle_in,
                        nodes[0].handle_out,
                        nodes[0].position,
                    );
                }
            }

            result.push(EditablePath {
                nodes,
                closed: subpath.closed,
            });
        }

        result
    }
}

/// Returns the index of the first draw command (LineTo/QuadTo/CubicTo) in a subpath,
/// scanning forward from index 0 and skipping MoveTo/Close.
fn first_draw_idx(subpath: &SubPath) -> Option<usize> {
    for i in 0..subpath.commands.len() {
        match subpath.commands[i] {
            PathCommand::LineTo { .. }
            | PathCommand::QuadTo { .. }
            | PathCommand::CubicTo { .. } => return Some(i),
            _ => {}
        }
    }
    None
}

/// Returns the index of the last draw command (LineTo/QuadTo/CubicTo) in a subpath,
/// scanning backward and skipping Close and MoveTo.
fn last_draw_idx(subpath: &SubPath) -> Option<usize> {
    for i in (0..subpath.commands.len()).rev() {
        match subpath.commands[i] {
            PathCommand::LineTo { .. }
            | PathCommand::QuadTo { .. }
            | PathCommand::CubicTo { .. } => return Some(i),
            _ => {}
        }
    }
    None
}

/// Returns the index of the next draw command after `cmd_idx`, wrapping around
/// for closed subpaths (skipping Close back to the first draw command after MoveTo).
fn next_draw_idx(subpath: &SubPath, cmd_idx: usize) -> Option<usize> {
    let next = cmd_idx + 1;
    if next < subpath.commands.len() {
        match subpath.commands[next] {
            PathCommand::LineTo { .. }
            | PathCommand::QuadTo { .. }
            | PathCommand::CubicTo { .. } => return Some(next),
            PathCommand::Close if subpath.closed => {
                return first_draw_idx(subpath);
            }
            _ => return None,
        }
    }
    // Out of bounds — wrap for closed paths
    if subpath.closed {
        first_draw_idx(subpath)
    } else {
        None
    }
}

/// Returns the index of the previous draw command before `cmd_idx`, scanning backward
/// and skipping Close/MoveTo. Wraps to the end for closed subpaths.
#[allow(dead_code)]
fn prev_draw_idx(subpath: &SubPath, cmd_idx: usize) -> Option<usize> {
    if cmd_idx == 0 {
        return if subpath.closed {
            last_draw_idx(subpath)
        } else {
            None
        };
    }
    for i in (0..cmd_idx).rev() {
        match subpath.commands[i] {
            PathCommand::LineTo { .. }
            | PathCommand::QuadTo { .. }
            | PathCommand::CubicTo { .. } => return Some(i),
            PathCommand::MoveTo { .. } | PathCommand::Close => {
                // Skip and keep scanning
            }
        }
    }
    // Reached index 0 without finding a draw command
    if subpath.closed {
        last_draw_idx(subpath)
    } else {
        None
    }
}

fn filter_zero_length_handle(handle: Option<Point2D>, node_pos: Point2D) -> Option<Point2D> {
    let handle = handle?;
    if (handle.x - node_pos.x).abs() < 1e-9 && (handle.y - node_pos.y).abs() < 1e-9 {
        None
    } else {
        Some(handle)
    }
}

/// Get the incoming bezier handle for a node.
fn get_incoming_handle(subpath: &SubPath, cmd_idx: usize, node_pos: Point2D) -> Option<Point2D> {
    // The incoming handle comes from the previous command's control points.
    // For a CubicTo, c2 is the incoming handle of the endpoint —
    // this is handled directly in the CubicTo branch of from_vecpath.
    // For MoveTo (cmd_idx == 0) in a closed path, the incoming handle
    // is c2/cx of the last draw command (the closing segment curves into MoveTo).
    if cmd_idx == 0 && subpath.closed {
        if let Some(last_idx) = last_draw_idx(subpath) {
            return match subpath.commands[last_idx] {
                PathCommand::CubicTo { c2x, c2y, .. } => {
                    filter_zero_length_handle(Some(Point2D::new(c2x, c2y)), node_pos)
                }
                PathCommand::QuadTo { cx, cy, .. } => {
                    filter_zero_length_handle(Some(Point2D::new(cx, cy)), node_pos)
                }
                _ => None,
            };
        }
    }
    None
}

/// Get the outgoing bezier handle for a node.
fn get_outgoing_handle(subpath: &SubPath, cmd_idx: usize, node_pos: Point2D) -> Option<Point2D> {
    // The outgoing handle comes from the next command's first control point.
    // Raw Close commands do not carry controls; materialized closing cubics are
    // represented explicitly before Close and are handled as a normal next draw.
    let Some(next_idx) = next_explicit_draw_idx(subpath, cmd_idx) else {
        return None;
    };
    match subpath.commands[next_idx] {
        PathCommand::CubicTo { c1x, c1y, .. } => {
            filter_zero_length_handle(Some(Point2D::new(c1x, c1y)), node_pos)
        }
        PathCommand::QuadTo { cx, cy, .. } => {
            filter_zero_length_handle(Some(Point2D::new(cx, cy)), node_pos)
        }
        _ => None,
    }
}

/// Move a node to a new position, adjusting handles relative to the movement.
pub fn move_node(path: &mut VecPath, node_id: NodeId, new_pos: Point2D) -> bool {
    let Some(subpath) = path.subpaths.get_mut(node_id.subpath_idx) else {
        return false;
    };
    let Some(cmd) = subpath.commands.get_mut(node_id.command_idx) else {
        return false;
    };

    let (old_x, old_y) = match *cmd {
        PathCommand::MoveTo { x, y }
        | PathCommand::LineTo { x, y }
        | PathCommand::QuadTo { x, y, .. }
        | PathCommand::CubicTo { x, y, .. } => (x, y),
        PathCommand::Close => return false,
    };

    let dx = new_pos.x - old_x;
    let dy = new_pos.y - old_y;

    // Move the endpoint
    match cmd {
        PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => {
            *x = new_pos.x;
            *y = new_pos.y;
        }
        PathCommand::QuadTo { x, y, cx, cy } => {
            *cx += dx;
            *cy += dy;
            *x = new_pos.x;
            *y = new_pos.y;
        }
        PathCommand::CubicTo { x, y, c2x, c2y, .. } => {
            *c2x += dx;
            *c2y += dy;
            *x = new_pos.x;
            *y = new_pos.y;
        }
        _ => return false,
    }

    // Also adjust the outgoing handle of this node (c1 of the next draw command),
    // wrapping for closed paths.
    if let Some(next_idx) = next_draw_idx(subpath, node_id.command_idx) {
        match &mut subpath.commands[next_idx] {
            PathCommand::CubicTo { c1x, c1y, .. } => {
                *c1x += dx;
                *c1y += dy;
            }
            PathCommand::QuadTo { cx, cy, .. } => {
                *cx += dx;
                *cy += dy;
            }
            _ => {}
        }
    }

    // For MoveTo (cmd_idx == 0) in a closed path, also adjust the incoming handle
    // (c2 of the last draw command) — it visually attaches to this node.
    if node_id.command_idx == 0 && subpath.closed {
        if let Some(last_idx) = last_draw_idx(subpath) {
            match &mut subpath.commands[last_idx] {
                PathCommand::CubicTo { c2x, c2y, .. } => {
                    *c2x += dx;
                    *c2y += dy;
                }
                PathCommand::QuadTo { cx, cy, .. } => {
                    *cx += dx;
                    *cy += dy;
                }
                _ => {}
            }
        }
    }

    // For MoveTo (cmd_idx == 0) in a closed path, if the last draw command's
    // endpoint coincides with the old MoveTo position (merged close-point),
    // update that endpoint too so it stays merged.
    if node_id.command_idx == 0 && subpath.closed {
        if let Some(last_idx) = last_draw_idx(subpath) {
            if let Some((lx, ly)) = endpoint_of(&subpath.commands[last_idx]) {
                if (lx - old_x).abs() < 1e-9 && (ly - old_y).abs() < 1e-9 {
                    match &mut subpath.commands[last_idx] {
                        PathCommand::CubicTo { x, y, .. }
                        | PathCommand::QuadTo { x, y, .. }
                        | PathCommand::LineTo { x, y } => {
                            *x = new_pos.x;
                            *y = new_pos.y;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    true
}

/// Move a bezier handle to a new position.
pub fn move_handle(
    path: &mut VecPath,
    node_id: NodeId,
    handle_type: HandleType,
    new_pos: Point2D,
) -> bool {
    let Some(subpath) = path.subpaths.get_mut(node_id.subpath_idx) else {
        return false;
    };

    match handle_type {
        HandleType::In => {
            // For MoveTo (cmd_idx == 0) in a closed path, the "in" handle is
            // c2/cx of the last draw command (the closing segment).
            if node_id.command_idx == 0 && subpath.closed {
                let Some(last_idx) = last_draw_idx(subpath) else {
                    return false;
                };
                let cmd = &mut subpath.commands[last_idx];
                return match cmd {
                    PathCommand::CubicTo { c2x, c2y, .. } => {
                        *c2x = new_pos.x;
                        *c2y = new_pos.y;
                        true
                    }
                    PathCommand::QuadTo { cx, cy, .. } => {
                        *cx = new_pos.x;
                        *cy = new_pos.y;
                        true
                    }
                    _ => false,
                };
            }
            // The "in" handle is c2 of the current CubicTo
            let Some(cmd) = subpath.commands.get_mut(node_id.command_idx) else {
                return false;
            };
            match cmd {
                PathCommand::CubicTo { c2x, c2y, .. } => {
                    *c2x = new_pos.x;
                    *c2y = new_pos.y;
                    true
                }
                PathCommand::QuadTo { cx, cy, .. } => {
                    *cx = new_pos.x;
                    *cy = new_pos.y;
                    true
                }
                _ => false,
            }
        }
        HandleType::Out => {
            // The "out" handle is c1 of the next draw command, wrapping for closed paths.
            let Some(next_idx) = next_draw_idx(subpath, node_id.command_idx) else {
                return false;
            };
            let cmd = &mut subpath.commands[next_idx];
            match cmd {
                PathCommand::CubicTo { c1x, c1y, .. } => {
                    *c1x = new_pos.x;
                    *c1y = new_pos.y;
                    true
                }
                PathCommand::QuadTo { cx, cy, .. } => {
                    *cx = new_pos.x;
                    *cy = new_pos.y;
                    true
                }
                _ => false,
            }
        }
    }
}

/// Delete a node from the path, reconnecting neighbors.
pub fn delete_node(path: &mut VecPath, node_id: NodeId) -> bool {
    let Some(subpath) = path.subpaths.get_mut(node_id.subpath_idx) else {
        return false;
    };
    if node_id.command_idx >= subpath.commands.len() {
        return false;
    }

    match subpath.commands[node_id.command_idx] {
        PathCommand::MoveTo { .. } => {
            let Some(next_idx) = next_explicit_draw_idx(subpath, node_id.command_idx) else {
                return false;
            };
            let Some((x, y)) = endpoint_of(&subpath.commands[next_idx]) else {
                return false;
            };
            subpath.commands[next_idx] = PathCommand::MoveTo { x, y };
            subpath.commands.remove(node_id.command_idx);
            true
        }
        PathCommand::Close => false,
        _ => {
            subpath.commands.remove(node_id.command_idx);
            true
        }
    }
}

/// Insert a node at parameter t on the segment at command_idx.
/// Splits a line into two lines, or a cubic into two cubics.
pub fn insert_node(path: &mut VecPath, node_id: NodeId, t: f64) -> bool {
    let t = t.clamp(0.01, 0.99);
    let Some(subpath) = path.subpaths.get_mut(node_id.subpath_idx) else {
        return false;
    };
    if node_id.command_idx >= subpath.commands.len() {
        return false;
    }

    // Get the start point (endpoint of previous command)
    let start = if node_id.command_idx > 0 {
        endpoint_of(&subpath.commands[node_id.command_idx - 1])
    } else {
        None
    };

    let Some(start) = start else {
        return false;
    };

    match subpath.commands[node_id.command_idx] {
        PathCommand::LineTo { x, y } => {
            let mid_x = start.0 + (x - start.0) * t;
            let mid_y = start.1 + (y - start.1) * t;

            subpath.commands[node_id.command_idx] = PathCommand::LineTo { x: mid_x, y: mid_y };
            subpath
                .commands
                .insert(node_id.command_idx + 1, PathCommand::LineTo { x, y });
            true
        }
        PathCommand::CubicTo {
            c1x,
            c1y,
            c2x,
            c2y,
            x,
            y,
        } => {
            let p0 = Point2D::new(start.0, start.1);
            let p1 = Point2D::new(c1x, c1y);
            let p2 = Point2D::new(c2x, c2y);
            let p3 = Point2D::new(x, y);

            let (before, after) = split_cubic(p0, p1, p2, p3, t);

            subpath.commands[node_id.command_idx] = PathCommand::CubicTo {
                c1x: before.1.x,
                c1y: before.1.y,
                c2x: before.2.x,
                c2y: before.2.y,
                x: before.3.x,
                y: before.3.y,
            };
            subpath.commands.insert(
                node_id.command_idx + 1,
                PathCommand::CubicTo {
                    c1x: after.1.x,
                    c1y: after.1.y,
                    c2x: after.2.x,
                    c2y: after.2.y,
                    x: after.3.x,
                    y: after.3.y,
                },
            );
            true
        }
        PathCommand::Close => {
            // Closing segment: implicit line from previous endpoint to MoveTo.
            // Materialize as two LineTos + Close, splitting at parameter t.
            let move_pos = match subpath.commands[0] {
                PathCommand::MoveTo { x, y } => (x, y),
                _ => return false,
            };
            let mid_x = start.0 + (move_pos.0 - start.0) * t;
            let mid_y = start.1 + (move_pos.1 - start.1) * t;

            // Replace Close with: LineTo(mid) + LineTo(move_pos) + Close
            subpath.commands[node_id.command_idx] = PathCommand::LineTo { x: mid_x, y: mid_y };
            subpath.commands.insert(
                node_id.command_idx + 1,
                PathCommand::LineTo {
                    x: move_pos.0,
                    y: move_pos.1,
                },
            );
            subpath
                .commands
                .insert(node_id.command_idx + 2, PathCommand::Close);
            true
        }
        _ => false,
    }
}

fn node_position(subpath: &SubPath, cmd_idx: usize) -> Option<Point2D> {
    match subpath.commands.get(cmd_idx)? {
        PathCommand::MoveTo { x, y }
        | PathCommand::LineTo { x, y }
        | PathCommand::QuadTo { x, y, .. }
        | PathCommand::CubicTo { x, y, .. } => Some(Point2D::new(*x, *y)),
        PathCommand::Close => None,
    }
}

fn incoming_segment_idx(subpath: &SubPath, cmd_idx: usize) -> Option<usize> {
    if cmd_idx == 0 {
        if subpath.closed {
            last_draw_idx(subpath)
        } else {
            None
        }
    } else {
        Some(cmd_idx)
    }
}

fn adjacent_points(
    subpath: &SubPath,
    cmd_idx: usize,
) -> (Option<Point2D>, Option<Point2D>, Point2D) {
    let node = node_position(subpath, cmd_idx).unwrap_or(Point2D::new(0.0, 0.0));
    let prev = if cmd_idx > 0 {
        endpoint_of(&subpath.commands[cmd_idx - 1]).map(|(x, y)| Point2D::new(x, y))
    } else if subpath.closed {
        last_draw_idx(subpath).and_then(|i| {
            endpoint_of(&subpath.commands[i]).and_then(|(x, y)| {
                let endpoint = Point2D::new(x, y);
                if points_coincident(endpoint, node) {
                    segment_start_point(subpath, i)
                } else {
                    Some(endpoint)
                }
            })
        })
    } else {
        None
    };
    let next =
        outgoing_segment_idx_for_node(subpath, cmd_idx).and_then(|i| match subpath.commands[i] {
            PathCommand::Close if subpath.closed => match subpath.commands[0] {
                PathCommand::MoveTo { x, y } => Some(Point2D::new(x, y)),
                _ => None,
            },
            _ => endpoint_of(&subpath.commands[i]).map(|(x, y)| Point2D::new(x, y)),
        });
    (prev, next, node)
}

fn next_explicit_draw_idx(subpath: &SubPath, cmd_idx: usize) -> Option<usize> {
    let next = cmd_idx + 1;
    if next >= subpath.commands.len() {
        return None;
    }
    match subpath.commands[next] {
        PathCommand::LineTo { .. } | PathCommand::QuadTo { .. } | PathCommand::CubicTo { .. } => {
            Some(next)
        }
        PathCommand::MoveTo { .. } | PathCommand::Close => None,
    }
}

fn close_command_idx(subpath: &SubPath) -> Option<usize> {
    subpath
        .commands
        .iter()
        .position(|cmd| matches!(cmd, PathCommand::Close))
}

fn points_coincident(a: Point2D, b: Point2D) -> bool {
    (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
}

fn lerp_point(a: Point2D, b: Point2D, t: f64) -> Point2D {
    Point2D::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

fn segment_start_point(subpath: &SubPath, cmd_idx: usize) -> Option<Point2D> {
    if cmd_idx == 0 {
        return None;
    }
    for i in (0..cmd_idx).rev() {
        if let Some((x, y)) = endpoint_of(&subpath.commands[i]) {
            return Some(Point2D::new(x, y));
        }
    }
    None
}

fn incoming_segment_idx_for_node(subpath: &SubPath, cmd_idx: usize) -> Option<usize> {
    if cmd_idx == 0 && subpath.closed {
        let node = node_position(subpath, cmd_idx)?;
        if let Some(last_idx) = last_draw_idx(subpath) {
            if endpoint_of(&subpath.commands[last_idx])
                .map(|(x, y)| points_coincident(Point2D::new(x, y), node))
                .unwrap_or(false)
            {
                return Some(last_idx);
            }
        }
        close_command_idx(subpath)
    } else {
        incoming_segment_idx(subpath, cmd_idx)
    }
}

fn outgoing_segment_idx_for_node(subpath: &SubPath, cmd_idx: usize) -> Option<usize> {
    let next = cmd_idx + 1;
    if next < subpath.commands.len() {
        match subpath.commands[next] {
            PathCommand::LineTo { .. }
            | PathCommand::QuadTo { .. }
            | PathCommand::CubicTo { .. }
            | PathCommand::Close => return Some(next),
            PathCommand::MoveTo { .. } => return None,
        }
    }
    if subpath.closed {
        close_command_idx(subpath)
    } else {
        None
    }
}

fn materialize_in_handle(path: &mut VecPath, node_id: NodeId, handle_pos: Point2D) -> bool {
    let Some(subpath) = path.subpaths.get(node_id.subpath_idx) else {
        return false;
    };
    let Some(seg_idx) = incoming_segment_idx_for_node(subpath, node_id.command_idx) else {
        return false;
    };
    let Some(node_pos) = node_position(subpath, node_id.command_idx) else {
        return false;
    };
    let Some(start) = segment_start_point(subpath, seg_idx) else {
        return false;
    };

    let subpath = &mut path.subpaths[node_id.subpath_idx];
    match subpath.commands[seg_idx] {
        PathCommand::LineTo { x, y } => {
            let c1 = lerp_point(start, node_pos, 1.0 / 3.0);
            subpath.commands[seg_idx] = PathCommand::CubicTo {
                c1x: c1.x,
                c1y: c1.y,
                c2x: handle_pos.x,
                c2y: handle_pos.y,
                x,
                y,
            };
            true
        }
        PathCommand::Close if subpath.closed => {
            let c1 = lerp_point(start, node_pos, 1.0 / 3.0);
            subpath.commands[seg_idx] = PathCommand::CubicTo {
                c1x: c1.x,
                c1y: c1.y,
                c2x: handle_pos.x,
                c2y: handle_pos.y,
                x: node_pos.x,
                y: node_pos.y,
            };
            subpath.commands.insert(seg_idx + 1, PathCommand::Close);
            true
        }
        _ => false,
    }
}

fn materialize_out_handle(path: &mut VecPath, node_id: NodeId, handle_pos: Point2D) -> bool {
    let Some(subpath) = path.subpaths.get(node_id.subpath_idx) else {
        return false;
    };
    let Some(seg_idx) = outgoing_segment_idx_for_node(subpath, node_id.command_idx) else {
        return false;
    };
    let Some(node_pos) = node_position(subpath, node_id.command_idx) else {
        return false;
    };
    let end = match subpath.commands[seg_idx] {
        PathCommand::LineTo { x, y } => Point2D::new(x, y),
        PathCommand::Close if subpath.closed => match subpath.commands[0] {
            PathCommand::MoveTo { x, y } => Point2D::new(x, y),
            _ => return false,
        },
        _ => return false,
    };

    let subpath = &mut path.subpaths[node_id.subpath_idx];
    match subpath.commands[seg_idx] {
        PathCommand::LineTo { x, y } => {
            let end = Point2D::new(x, y);
            let c2 = lerp_point(node_pos, end, 2.0 / 3.0);
            subpath.commands[seg_idx] = PathCommand::CubicTo {
                c1x: handle_pos.x,
                c1y: handle_pos.y,
                c2x: c2.x,
                c2y: c2.y,
                x,
                y,
            };
            true
        }
        PathCommand::Close if subpath.closed => {
            let c2 = lerp_point(node_pos, end, 2.0 / 3.0);
            subpath.commands[seg_idx] = PathCommand::CubicTo {
                c1x: handle_pos.x,
                c1y: handle_pos.y,
                c2x: c2.x,
                c2y: c2.y,
                x: end.x,
                y: end.y,
            };
            subpath.commands.insert(seg_idx + 1, PathCommand::Close);
            true
        }
        _ => false,
    }
}

fn smooth_tangent(prev: Option<Point2D>, next: Option<Point2D>, node: Point2D) -> Option<Point2D> {
    let dir = match (prev, next) {
        (Some(prev), Some(next)) => Point2D::new(next.x - prev.x, next.y - prev.y),
        (Some(prev), None) => Point2D::new(node.x - prev.x, node.y - prev.y),
        (None, Some(next)) => Point2D::new(next.x - node.x, next.y - node.y),
        (None, None) => return None,
    };
    let len = (dir.x * dir.x + dir.y * dir.y).sqrt();
    if len < 1e-9 {
        None
    } else {
        Some(Point2D::new(dir.x / len, dir.y / len))
    }
}

fn smooth_handle_positions(
    subpath: &SubPath,
    cmd_idx: usize,
) -> (Option<Point2D>, Option<Point2D>) {
    let (prev, next, node) = adjacent_points(subpath, cmd_idx);
    let Some(tangent) = smooth_tangent(prev, next, node) else {
        return (None, None);
    };
    let in_handle = prev.map(|prev| {
        let len = ((node.x - prev.x).powi(2) + (node.y - prev.y).powi(2)).sqrt() / 3.0;
        Point2D::new(node.x - tangent.x * len, node.y - tangent.y * len)
    });
    let out_handle = next.map(|next| {
        let len = ((next.x - node.x).powi(2) + (next.y - node.y).powi(2)).sqrt() / 3.0;
        Point2D::new(node.x + tangent.x * len, node.y + tangent.y * len)
    });
    (in_handle, out_handle)
}

/// Set a node's type by mutating durable path geometry.
pub fn set_node_type(path: &mut VecPath, node_id: NodeId, node_type: NodeType) -> bool {
    let Some(subpath) = path.subpaths.get(node_id.subpath_idx) else {
        return false;
    };
    if node_id.command_idx >= subpath.commands.len() {
        return false;
    }
    let Some(node_pos) = node_position(subpath, node_id.command_idx) else {
        return false;
    };

    match node_type {
        NodeType::Corner => {
            let mut changed = false;
            changed |= move_handle(path, node_id, HandleType::In, node_pos);
            changed |= move_handle(path, node_id, HandleType::Out, node_pos);
            changed
        }
        NodeType::Smooth => {
            let (desired_in, desired_out) = {
                let subpath = &path.subpaths[node_id.subpath_idx];
                smooth_handle_positions(subpath, node_id.command_idx)
            };
            let mut changed = false;

            if let Some(handle_pos) = desired_in {
                let needs_materialized_close = incoming_segment_idx_for_node(
                    &path.subpaths[node_id.subpath_idx],
                    node_id.command_idx,
                )
                .map(|idx| {
                    matches!(
                        path.subpaths[node_id.subpath_idx].commands[idx],
                        PathCommand::Close
                    )
                })
                .unwrap_or(false);
                if needs_materialized_close
                    || !move_handle(path, node_id, HandleType::In, handle_pos)
                {
                    if !materialize_in_handle(path, node_id, handle_pos) {
                        return false;
                    }
                }
                changed = true;
            }

            if let Some(handle_pos) = desired_out {
                let needs_materialized_close = outgoing_segment_idx_for_node(
                    &path.subpaths[node_id.subpath_idx],
                    node_id.command_idx,
                )
                .map(|idx| {
                    matches!(
                        path.subpaths[node_id.subpath_idx].commands[idx],
                        PathCommand::Close
                    )
                })
                .unwrap_or(false);
                if needs_materialized_close
                    || !move_handle(path, node_id, HandleType::Out, handle_pos)
                {
                    if !materialize_out_handle(path, node_id, handle_pos) {
                        return false;
                    }
                }
                changed = true;
            }

            changed
        }
    }
}

/// Replace a CubicTo, QuadTo, or Close with a LineTo at the same endpoint (strips handles).
/// For Close on a closed path, materializes the implicit closing line as an explicit LineTo.
/// Returns false if the command is already a LineTo or is not convertible.
pub fn convert_segment_to_line(path: &mut VecPath, node_id: NodeId) -> bool {
    let Some(subpath) = path.subpaths.get_mut(node_id.subpath_idx) else {
        return false;
    };
    let Some(cmd) = subpath.commands.get(node_id.command_idx) else {
        return false;
    };
    match *cmd {
        PathCommand::CubicTo { x, y, .. } | PathCommand::QuadTo { x, y, .. } => {
            subpath.commands[node_id.command_idx] = PathCommand::LineTo { x, y };
            true
        }
        PathCommand::Close if subpath.closed => {
            let move_pos = match subpath.commands[0] {
                PathCommand::MoveTo { x, y } => (x, y),
                _ => return false,
            };
            subpath.commands[node_id.command_idx] = PathCommand::LineTo {
                x: move_pos.0,
                y: move_pos.1,
            };
            subpath.commands.push(PathCommand::Close);
            true
        }
        _ => false,
    }
}

/// Replace a LineTo (or Close on a closed path) with a CubicTo, placing control
/// points at 1/3 and 2/3 between the previous endpoint and this endpoint.
/// Returns false if the command is already a CubicTo/QuadTo or is not convertible.
pub fn convert_segment_to_curve(path: &mut VecPath, node_id: NodeId) -> bool {
    let Some(subpath) = path.subpaths.get(node_id.subpath_idx) else {
        return false;
    };
    if node_id.command_idx >= subpath.commands.len() {
        return false;
    }

    // Determine endpoint — LineTo has it directly, Close targets the MoveTo position
    let (x, y, is_close) = match subpath.commands[node_id.command_idx] {
        PathCommand::LineTo { x, y } => (x, y, false),
        PathCommand::Close if subpath.closed => match subpath.commands[0] {
            PathCommand::MoveTo { x, y } => (x, y, true),
            _ => return false,
        },
        _ => return false,
    };

    // Get the start point (endpoint of previous command)
    let start = if node_id.command_idx > 0 {
        endpoint_of(&subpath.commands[node_id.command_idx - 1])
    } else if subpath.closed {
        last_draw_idx(subpath).and_then(|i| endpoint_of(&subpath.commands[i]))
    } else {
        None
    };

    let Some((sx, sy)) = start else {
        return false;
    };

    let subpath = &mut path.subpaths[node_id.subpath_idx];
    subpath.commands[node_id.command_idx] = PathCommand::CubicTo {
        c1x: sx + (x - sx) / 3.0,
        c1y: sy + (y - sy) / 3.0,
        c2x: sx + (x - sx) * 2.0 / 3.0,
        c2y: sy + (y - sy) * 2.0 / 3.0,
        x,
        y,
    };
    if is_close {
        // Re-add Close after the materialized CubicTo
        subpath.commands.push(PathCommand::Close);
    }
    true
}

/// Delete the segment at `command_idx`.
/// - On a closed subpath: opens the path at the deleted edge, preserving all nodes.
/// - On an open subpath: splits into two subpaths at the gap.
/// Returns false if the command is MoveTo, or Close on an open path, or indices are invalid.
pub fn delete_segment(path: &mut VecPath, node_id: NodeId) -> bool {
    let Some(subpath) = path.subpaths.get(node_id.subpath_idx) else {
        return false;
    };
    if node_id.command_idx >= subpath.commands.len() {
        return false;
    }
    match subpath.commands[node_id.command_idx] {
        PathCommand::MoveTo { .. } => return false,
        PathCommand::Close if !subpath.closed => return false,
        _ => {}
    }

    if subpath.closed {
        if matches!(subpath.commands[node_id.command_idx], PathCommand::Close) {
            // Closing segment: just open the path, all nodes preserved
            let subpath = &mut path.subpaths[node_id.subpath_idx];
            subpath
                .commands
                .retain(|c| !matches!(c, PathCommand::Close));
            subpath.closed = false;
            return true;
        }

        // Non-Close segment: rotate so the deleted edge's destination becomes the
        // new start, then open. All nodes are preserved; the gap replaces the deleted edge.
        let cmd_idx = node_id.command_idx;
        let break_pos = match subpath.commands[cmd_idx] {
            PathCommand::LineTo { x, y }
            | PathCommand::QuadTo { x, y, .. }
            | PathCommand::CubicTo { x, y, .. } => (x, y),
            _ => return false,
        };

        let orig_move = match subpath.commands[0] {
            PathCommand::MoveTo { x, y } => (x, y),
            _ => return false,
        };

        let after: Vec<PathCommand> = subpath.commands[cmd_idx + 1..]
            .iter()
            .filter(|c| !matches!(c, PathCommand::Close))
            .cloned()
            .collect();

        let before: Vec<PathCommand> = subpath.commands[1..cmd_idx].to_vec();

        let mut new_cmds = vec![PathCommand::MoveTo {
            x: break_pos.0,
            y: break_pos.1,
        }];
        new_cmds.extend(after);
        new_cmds.push(PathCommand::LineTo {
            x: orig_move.0,
            y: orig_move.1,
        });
        new_cmds.extend(before);

        let subpath = &mut path.subpaths[node_id.subpath_idx];
        subpath.commands = new_cmds;
        subpath.closed = false;
        true
    } else {
        // Split into two subpaths
        let subpath = &path.subpaths[node_id.subpath_idx];
        let cmd_idx = node_id.command_idx;

        let first_cmds: Vec<PathCommand> = subpath.commands[..cmd_idx].to_vec();
        let deleted_endpoint = endpoint_of(&subpath.commands[cmd_idx]);
        let rest_cmds: Vec<PathCommand> = subpath.commands[cmd_idx + 1..].to_vec();

        let Some((ex, ey)) = deleted_endpoint else {
            return false;
        };

        let mut second_cmds = vec![PathCommand::MoveTo { x: ex, y: ey }];
        second_cmds.extend(rest_cmds);

        let sp_idx = node_id.subpath_idx;
        path.subpaths[sp_idx] = SubPath {
            commands: first_cmds,
            closed: false,
        };
        path.subpaths.insert(
            sp_idx + 1,
            SubPath {
                commands: second_cmds,
                closed: false,
            },
        );
        true
    }
}

/// Break/split a path at a specific node.
/// - On a closed subpath: rotates the commands so this node is the start, then opens.
/// - On an open subpath: splits into two subpaths at this node (node appears in both).
/// Returns false if indices are invalid.
pub fn break_path_at_node(path: &mut VecPath, node_id: NodeId) -> bool {
    let Some(subpath) = path.subpaths.get(node_id.subpath_idx) else {
        return false;
    };
    if node_id.command_idx >= subpath.commands.len() {
        return false;
    }

    if subpath.closed {
        let cmd_idx = node_id.command_idx;

        let break_pos = match subpath.commands[cmd_idx] {
            PathCommand::MoveTo { x, y }
            | PathCommand::LineTo { x, y }
            | PathCommand::QuadTo { x, y, .. }
            | PathCommand::CubicTo { x, y, .. } => Point2D::new(x, y),
            PathCommand::Close => return false,
        };

        let orig_move = match subpath.commands[0] {
            PathCommand::MoveTo { x, y } => Point2D::new(x, y),
            _ => return false,
        };

        let has_explicit_closing_draw = last_draw_idx(subpath)
            .and_then(|i| endpoint_of(&subpath.commands[i]))
            .map(|(x, y)| points_coincident(Point2D::new(x, y), orig_move))
            .unwrap_or(false);

        // Rotate and open without dropping geometry:
        // [M, c1..ci, ci+1..cn, Z] -> [M(ci), ci+1..cn, M-closing-edge, c1..ci]
        // The inclusive c1..ci tail preserves the segment into the break node.
        let after: Vec<PathCommand> = subpath.commands[cmd_idx + 1..]
            .iter()
            .filter(|c| !matches!(c, PathCommand::Close))
            .cloned()
            .collect();
        let before_inclusive: Vec<PathCommand> = if cmd_idx == 0 {
            Vec::new()
        } else {
            subpath.commands[1..=cmd_idx].to_vec()
        };

        let mut new_cmds = vec![PathCommand::MoveTo {
            x: break_pos.x,
            y: break_pos.y,
        }];
        new_cmds.extend(after);
        if !has_explicit_closing_draw {
            new_cmds.push(PathCommand::LineTo {
                x: orig_move.x,
                y: orig_move.y,
            });
        }
        new_cmds.extend(before_inclusive);

        let sp = &mut path.subpaths[node_id.subpath_idx];
        sp.commands = new_cmds;
        sp.closed = false;
        true
    } else {
        let cmd_idx = node_id.command_idx;

        match subpath.commands[cmd_idx] {
            PathCommand::MoveTo { .. } => return cmd_idx == 0,
            PathCommand::Close => return false,
            _ => {}
        }

        let endpoint = endpoint_of(&subpath.commands[cmd_idx]);
        let Some((ex, ey)) = endpoint else {
            return false;
        };

        let first_cmds: Vec<PathCommand> = subpath.commands[..=cmd_idx].to_vec();
        let rest: Vec<PathCommand> = subpath.commands[cmd_idx + 1..].to_vec();
        let mut second_cmds = vec![PathCommand::MoveTo { x: ex, y: ey }];
        second_cmds.extend(rest);

        let sp_idx = node_id.subpath_idx;
        path.subpaths[sp_idx] = SubPath {
            commands: first_cmds,
            closed: false,
        };
        path.subpaths.insert(
            sp_idx + 1,
            SubPath {
                commands: second_cmds,
                closed: false,
            },
        );
        true
    }
}

/// Toggle a subpath between open and closed.
/// If open: appends Close and sets `closed = true`.
/// If closed: removes Close and sets `closed = false`.
pub fn toggle_path_closed(path: &mut VecPath, subpath_idx: usize) -> bool {
    let Some(subpath) = path.subpaths.get_mut(subpath_idx) else {
        return false;
    };

    if subpath.closed {
        subpath
            .commands
            .retain(|c| !matches!(c, PathCommand::Close));
        subpath.closed = false;
    } else {
        subpath
            .commands
            .retain(|c| !matches!(c, PathCommand::Close));
        subpath.commands.push(PathCommand::Close);
        subpath.closed = true;
    }
    true
}

fn endpoint_of(cmd: &PathCommand) -> Option<(f64, f64)> {
    match *cmd {
        PathCommand::MoveTo { x, y }
        | PathCommand::LineTo { x, y }
        | PathCommand::QuadTo { x, y, .. }
        | PathCommand::CubicTo { x, y, .. } => Some((x, y)),
        PathCommand::Close => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editable_path_from_simple_path() {
        let path = VecPath::parse_svg_d("M0 0 L10 10 L20 0 Z");
        let editable = EditablePath::from_vecpath(&path);
        assert_eq!(editable.len(), 1);
        assert_eq!(editable[0].nodes.len(), 3); // M, L, L (Close doesn't create a node)
        assert!(editable[0].closed);
    }

    #[test]
    fn editable_path_from_cubic() {
        let path = VecPath::parse_svg_d("M0 0 C10 20 30 40 50 0");
        let editable = EditablePath::from_vecpath(&path);
        assert_eq!(editable[0].nodes.len(), 2); // M and C endpoint
        // The cubic endpoint should have an incoming handle
        assert!(editable[0].nodes[1].handle_in.is_some());
    }

    #[test]
    fn move_node_updates_position() {
        let mut path = VecPath::parse_svg_d("M0 0 L10 10 L20 0");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 1,
        };
        assert!(move_node(&mut path, node_id, Point2D::new(15.0, 15.0)));
        assert_eq!(
            path.subpaths[0].commands[1],
            PathCommand::LineTo { x: 15.0, y: 15.0 }
        );
    }

    #[test]
    fn delete_node_removes_command() {
        let mut path = VecPath::parse_svg_d("M0 0 L10 10 L20 0 L30 10");
        let original_len = path.subpaths[0].commands.len();
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 1,
        };
        assert!(delete_node(&mut path, node_id));
        assert_eq!(path.subpaths[0].commands.len(), original_len - 1);
    }

    #[test]
    fn delete_first_open_node_promotes_next_node_to_moveto() {
        let mut path = VecPath::parse_svg_d("M0 0 L10 10 L20 0");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 0,
        };

        assert!(delete_node(&mut path, node_id));
        assert_eq!(
            path.subpaths[0].commands,
            vec![
                PathCommand::MoveTo { x: 10.0, y: 10.0 },
                PathCommand::LineTo { x: 20.0, y: 0.0 },
            ],
        );
    }

    #[test]
    fn delete_first_closed_node_promotes_next_node_to_moveto() {
        let mut path = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 0,
        };

        assert!(delete_node(&mut path, node_id));
        assert!(path.subpaths[0].closed);
        assert_eq!(
            path.subpaths[0].commands,
            vec![
                PathCommand::MoveTo { x: 10.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 10.0 },
                PathCommand::LineTo { x: 0.0, y: 10.0 },
                PathCommand::Close,
            ],
        );
    }

    #[test]
    fn cannot_delete_only_moveto() {
        let mut path = VecPath::parse_svg_d("M0 0 L10 10");
        path.subpaths[0].commands.truncate(1);
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 0,
        };
        assert!(!delete_node(&mut path, node_id));
    }

    #[test]
    fn insert_node_on_line() {
        let mut path = VecPath::parse_svg_d("M0 0 L10 10");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 1,
        };
        assert!(insert_node(&mut path, node_id, 0.5));
        assert_eq!(path.subpaths[0].commands.len(), 3);

        // Check the midpoint
        if let PathCommand::LineTo { x, y } = path.subpaths[0].commands[1] {
            assert!((x - 5.0).abs() < 1e-10);
            assert!((y - 5.0).abs() < 1e-10);
        } else {
            panic!("Expected LineTo");
        }
    }

    #[test]
    fn insert_node_on_cubic() {
        let mut path = VecPath::parse_svg_d("M0 0 C10 20 30 20 40 0");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 1,
        };
        assert!(insert_node(&mut path, node_id, 0.5));
        assert_eq!(path.subpaths[0].commands.len(), 3);
        // Both should be cubics now
        assert!(matches!(
            path.subpaths[0].commands[1],
            PathCommand::CubicTo { .. }
        ));
        assert!(matches!(
            path.subpaths[0].commands[2],
            PathCommand::CubicTo { .. }
        ));
    }

    #[test]
    fn move_handle_in() {
        let mut path = VecPath::parse_svg_d("M0 0 C10 20 30 40 50 0");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 1,
        };
        assert!(move_handle(
            &mut path,
            node_id,
            HandleType::In,
            Point2D::new(35.0, 45.0)
        ));
        if let PathCommand::CubicTo { c2x, c2y, .. } = path.subpaths[0].commands[1] {
            assert_eq!(c2x, 35.0);
            assert_eq!(c2y, 45.0);
        } else {
            panic!("Expected CubicTo");
        }
    }

    #[test]
    fn editable_path_node_ids_are_correct() {
        let path = VecPath::parse_svg_d("M0 0 L10 10 L20 0");
        let editable = EditablePath::from_vecpath(&path);
        assert_eq!(editable[0].nodes[0].id.command_idx, 0);
        assert_eq!(editable[0].nodes[1].id.command_idx, 1);
        assert_eq!(editable[0].nodes[2].id.command_idx, 2);
    }

    // --- Closed-path seam handling tests ---

    #[test]
    fn closed_path_incoming_handle_on_moveto() {
        // M0 0 L10 0 C15 5 5 -5 0 0 Z
        // Node 0 (MoveTo) should have handle_in from c2 of the last CubicTo = (5, -5)
        let path = VecPath::parse_svg_d("M0 0 L10 0 C15 5 5 -5 0 0 Z");
        let editable = EditablePath::from_vecpath(&path);
        let node0 = &editable[0].nodes[0];
        assert_eq!(node0.handle_in, Some(Point2D::new(5.0, -5.0)));
    }

    #[test]
    fn closed_path_outgoing_handle_on_last_node() {
        // M0 0 C5 10 15 10 20 0 C15 -5 5 -5 0 0 Z
        // After merge (last CubicTo endpoint (0,0) == MoveTo (0,0)), there are 2 nodes:
        //   node 0 = MoveTo (0,0), node 1 = CubicTo (20,0)
        // Last node (cmd 1) has handle_out = c1 of cmd 2 = (15, -5)
        let path = VecPath::parse_svg_d("M0 0 C5 10 15 10 20 0 C15 -5 5 -5 0 0 Z");
        let editable = EditablePath::from_vecpath(&path);
        assert_eq!(editable[0].nodes.len(), 2); // merged from 3 → 2
        let last = editable[0].nodes.last().unwrap();
        assert_eq!(last.handle_out, Some(Point2D::new(15.0, -5.0)));
    }

    #[test]
    fn closed_path_line_only_no_seam_handles() {
        let path = VecPath::parse_svg_d("M0 0 L10 0 L20 0 Z");
        let editable = EditablePath::from_vecpath(&path);
        for node in &editable[0].nodes {
            assert_eq!(node.handle_in, None);
            assert_eq!(node.handle_out, None);
        }
    }

    #[test]
    fn raw_close_does_not_wrap_first_curve_handle_to_last_node() {
        let path = VecPath::parse_svg_d("M0 0 C5 0 5 10 10 10 L0 10 Z");
        let editable = EditablePath::from_vecpath(&path);
        let last = editable[0].nodes.last().unwrap();
        assert_eq!(last.position, Point2D::new(0.0, 10.0));
        assert_eq!(last.handle_out, None);
        assert_eq!(last.node_type, NodeType::Corner);
    }

    #[test]
    fn move_node_closed_path_wraps_outgoing_handle() {
        // M0 0 C5 10 15 10 20 0 L10 -5 Z
        // Move last node (cmd 2, LineTo at (10,-5)) by (+2, +3)
        // → c1 of cmd 1 (the CubicTo) should be displaced: (5+2, 10+3) = (7, 13)
        let mut path = VecPath::parse_svg_d("M0 0 C5 10 15 10 20 0 L10 -5 Z");
        let last_cmd_idx = 2; // LineTo
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: last_cmd_idx,
        };
        assert!(move_node(&mut path, node_id, Point2D::new(12.0, -2.0)));
        // c1 of cmd 1 (wrapping: next_draw_idx of cmd 2 → cmd 1)
        if let PathCommand::CubicTo { c1x, c1y, .. } = path.subpaths[0].commands[1] {
            assert!((c1x - 7.0).abs() < 1e-10);
            assert!((c1y - 13.0).abs() < 1e-10);
        } else {
            panic!("Expected CubicTo at cmd 1");
        }
    }

    #[test]
    fn move_node_moveto_closed_path_adjusts_incoming() {
        // M0 0 L10 0 C15 5 5 -5 0 0 Z
        // Move node 0 (MoveTo) by (+3, +4)
        // → c2 of last CubicTo (cmd 2) should be displaced: (5+3, -5+4) = (8, -1)
        let mut path = VecPath::parse_svg_d("M0 0 L10 0 C15 5 5 -5 0 0 Z");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 0,
        };
        assert!(move_node(&mut path, node_id, Point2D::new(3.0, 4.0)));
        if let PathCommand::CubicTo { c2x, c2y, .. } = path.subpaths[0].commands[2] {
            assert!((c2x - 8.0).abs() < 1e-10);
            assert!((c2y - (-1.0)).abs() < 1e-10);
        } else {
            panic!("Expected CubicTo at cmd 2");
        }
    }

    #[test]
    fn move_handle_in_on_moveto_closed_path() {
        // M0 0 L10 0 C15 5 5 -5 0 0 Z
        // move_handle(In) on node 0 → should modify c2 of last CubicTo (cmd 2)
        let mut path = VecPath::parse_svg_d("M0 0 L10 0 C15 5 5 -5 0 0 Z");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 0,
        };
        assert!(move_handle(
            &mut path,
            node_id,
            HandleType::In,
            Point2D::new(2.0, -3.0)
        ));
        if let PathCommand::CubicTo { c2x, c2y, .. } = path.subpaths[0].commands[2] {
            assert_eq!(c2x, 2.0);
            assert_eq!(c2y, -3.0);
        } else {
            panic!("Expected CubicTo at cmd 2");
        }
    }

    #[test]
    fn move_handle_out_on_last_node_closed_path() {
        // M0 0 C5 10 15 10 20 0 C15 -5 5 -5 0 0 Z
        // move_handle(Out) on last node (cmd 2) → should modify c1 of cmd 1 (wrapping)
        let mut path = VecPath::parse_svg_d("M0 0 C5 10 15 10 20 0 C15 -5 5 -5 0 0 Z");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 2,
        };
        assert!(move_handle(
            &mut path,
            node_id,
            HandleType::Out,
            Point2D::new(7.0, 12.0)
        ));
        if let PathCommand::CubicTo { c1x, c1y, .. } = path.subpaths[0].commands[1] {
            assert_eq!(c1x, 7.0);
            assert_eq!(c1y, 12.0);
        } else {
            panic!("Expected CubicTo at cmd 1");
        }
    }

    #[test]
    fn open_path_not_affected_by_wrapping() {
        // Open path: last node should have handle_out = None when no next cmd
        let path = VecPath::parse_svg_d("M0 0 C5 10 15 10 20 0");
        let editable = EditablePath::from_vecpath(&path);
        let last = editable[0].nodes.last().unwrap();
        assert_eq!(last.handle_out, None);
        // Also node 0 should have handle_in = None (not closed)
        assert_eq!(editable[0].nodes[0].handle_in, None);
    }

    #[test]
    fn helper_next_draw_idx_basics() {
        let path = VecPath::parse_svg_d("M0 0 C5 10 15 10 20 0 L30 5 Z");
        let sp = &path.subpaths[0];
        // cmd 0 = MoveTo, cmd 1 = CubicTo, cmd 2 = LineTo, cmd 3 = Close
        assert_eq!(next_draw_idx(sp, 0), Some(1)); // MoveTo → CubicTo
        assert_eq!(next_draw_idx(sp, 1), Some(2)); // CubicTo → LineTo
        assert_eq!(next_draw_idx(sp, 2), Some(1)); // LineTo → Close → wraps to 1

        // Open path: no wrapping
        let open = VecPath::parse_svg_d("M0 0 L10 0 L20 0");
        let sp_open = &open.subpaths[0];
        assert_eq!(next_draw_idx(sp_open, 2), None); // Last cmd, no wrap

        // Degenerate closed subpath with no draw commands (M … Z)
        let degen = SubPath {
            commands: vec![PathCommand::MoveTo { x: 0.0, y: 0.0 }, PathCommand::Close],
            closed: true,
        };
        assert_eq!(next_draw_idx(&degen, 0), None); // No draw command to wrap to
    }

    #[test]
    fn helper_prev_draw_idx_basics() {
        let path = VecPath::parse_svg_d("M0 0 C5 10 15 10 20 0 L30 5 Z");
        let sp = &path.subpaths[0];
        // cmd 0 = MoveTo, cmd 1 = CubicTo, cmd 2 = LineTo, cmd 3 = Close
        assert_eq!(prev_draw_idx(sp, 2), Some(1)); // LineTo ← CubicTo
        assert_eq!(prev_draw_idx(sp, 1), Some(2)); // CubicTo ← wraps to LineTo (closed)
        assert_eq!(prev_draw_idx(sp, 0), Some(2)); // MoveTo ← wraps to LineTo (closed)

        // Open path: no wrapping
        let open = VecPath::parse_svg_d("M0 0 L10 0 L20 0");
        let sp_open = &open.subpaths[0];
        assert_eq!(prev_draw_idx(sp_open, 1), None); // cmd 1 ← cmd 0 is MoveTo, not closed → None
        assert_eq!(prev_draw_idx(sp_open, 0), None); // cmd 0, not closed → None
    }

    // --- Coincident close-point merge tests ---

    #[test]
    fn closed_path_merges_coincident_endpoint() {
        // Simulate an ellipse: M cx cy-ry  C... C... C... C(endpoint==start) Z
        // The last CubicTo endpoint matches the MoveTo start → should merge.
        let path = VecPath {
            subpaths: vec![SubPath {
                commands: vec![
                    PathCommand::MoveTo { x: 50.0, y: 0.0 },
                    PathCommand::CubicTo {
                        c1x: 77.6,
                        c1y: 0.0,
                        c2x: 100.0,
                        c2y: 22.4,
                        x: 100.0,
                        y: 50.0,
                    },
                    PathCommand::CubicTo {
                        c1x: 100.0,
                        c1y: 77.6,
                        c2x: 77.6,
                        c2y: 100.0,
                        x: 50.0,
                        y: 100.0,
                    },
                    PathCommand::CubicTo {
                        c1x: 22.4,
                        c1y: 100.0,
                        c2x: 0.0,
                        c2y: 77.6,
                        x: 0.0,
                        y: 50.0,
                    },
                    PathCommand::CubicTo {
                        c1x: 0.0,
                        c1y: 22.4,
                        c2x: 22.4,
                        c2y: 0.0,
                        x: 50.0,
                        y: 0.0, // == MoveTo start
                    },
                    PathCommand::Close,
                ],
                closed: true,
            }],
        };
        let editable = EditablePath::from_vecpath(&path);
        assert_eq!(editable.len(), 1);
        // Should be 4 nodes (not 5) — last merged into first
        assert_eq!(editable[0].nodes.len(), 4);
        // First node should have handle_in from the removed last cubic's c2
        assert_eq!(
            editable[0].nodes[0].handle_in,
            Some(Point2D::new(22.4, 0.0))
        );
        assert_eq!(editable[0].nodes[0].node_type, NodeType::Smooth);
    }

    #[test]
    fn closed_path_no_merge_when_not_coincident() {
        // Polygon: M + 3L + Z, last L endpoint != M start → no merge
        let path = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let editable = EditablePath::from_vecpath(&path);
        assert_eq!(editable[0].nodes.len(), 4); // M, L, L, L — all distinct
    }

    #[test]
    fn move_node_updates_coincident_close_endpoint() {
        // Ellipse-like path where last CubicTo endpoint == MoveTo start
        let mut path = VecPath {
            subpaths: vec![SubPath {
                commands: vec![
                    PathCommand::MoveTo { x: 50.0, y: 0.0 },
                    PathCommand::CubicTo {
                        c1x: 77.6,
                        c1y: 0.0,
                        c2x: 100.0,
                        c2y: 22.4,
                        x: 100.0,
                        y: 50.0,
                    },
                    PathCommand::CubicTo {
                        c1x: 100.0,
                        c1y: 77.6,
                        c2x: 77.6,
                        c2y: 100.0,
                        x: 50.0,
                        y: 100.0,
                    },
                    PathCommand::CubicTo {
                        c1x: 22.4,
                        c1y: 100.0,
                        c2x: 0.0,
                        c2y: 77.6,
                        x: 0.0,
                        y: 50.0,
                    },
                    PathCommand::CubicTo {
                        c1x: 0.0,
                        c1y: 22.4,
                        c2x: 22.4,
                        c2y: 0.0,
                        x: 50.0,
                        y: 0.0, // coincident with MoveTo
                    },
                    PathCommand::Close,
                ],
                closed: true,
            }],
        };

        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 0,
        };
        assert!(move_node(&mut path, node_id, Point2D::new(55.0, 5.0)));

        // MoveTo should be updated
        assert!(matches!(
            path.subpaths[0].commands[0],
            PathCommand::MoveTo { x, y } if (x - 55.0).abs() < 1e-9 && (y - 5.0).abs() < 1e-9
        ));
        // Last CubicTo endpoint should also be updated to match
        if let PathCommand::CubicTo { x, y, .. } = path.subpaths[0].commands[4] {
            assert!(
                (x - 55.0).abs() < 1e-9 && (y - 5.0).abs() < 1e-9,
                "Last CubicTo endpoint should follow MoveTo: got ({x}, {y})"
            );
        } else {
            panic!("Expected CubicTo at cmd 4");
        }
    }

    #[test]
    fn insert_node_on_closing_segment() {
        // Rectangle: M0 0 L10 0 L10 10 L0 10 Z
        // Insert on Close (cmd 4) — the left side from (0,10) back to (0,0)
        let mut path = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        assert_eq!(path.subpaths[0].commands.len(), 5); // M, L, L, L, Z
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 4, // Close command
        };
        assert!(insert_node(&mut path, node_id, 0.5));
        // Should now have: M0 0, L10 0, L10 10, L0 10, L0 5, L0 0, Z
        assert_eq!(path.subpaths[0].commands.len(), 7);
        assert!(path.subpaths[0].closed);
        // Check midpoint at (0, 5)
        if let PathCommand::LineTo { x, y } = path.subpaths[0].commands[4] {
            assert!((x - 0.0).abs() < 1e-9);
            assert!((y - 5.0).abs() < 1e-9);
        } else {
            panic!("Expected LineTo at cmd 4");
        }
    }

    #[test]
    fn zero_length_handles_round_trip_as_corner() {
        let path = VecPath::parse_svg_d("M0 0 C0 0 10 0 10 0");
        let editable = EditablePath::from_vecpath(&path);
        assert_eq!(editable[0].nodes.len(), 2);
        assert_eq!(editable[0].nodes[0].node_type, NodeType::Corner);
        assert_eq!(editable[0].nodes[1].node_type, NodeType::Corner);
        assert!(editable[0].nodes[0].handle_out.is_none());
        assert!(editable[0].nodes[1].handle_in.is_none());
    }

    #[test]
    fn set_node_type_corner_collapses_adjacent_handles() {
        let mut path = VecPath::parse_svg_d("M0 0 C10 0 20 10 30 10 C40 10 50 0 60 0");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 1,
        };
        assert!(set_node_type(&mut path, node_id, NodeType::Corner));
        let editable = EditablePath::from_vecpath(&path);
        assert_eq!(editable[0].nodes[1].node_type, NodeType::Corner);
        assert!(editable[0].nodes[1].handle_in.is_none());
        assert!(editable[0].nodes[1].handle_out.is_none());
    }

    #[test]
    fn set_node_type_smooth_materializes_handles() {
        let mut path = VecPath::parse_svg_d("M0 0 L30 10 L60 0");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 1,
        };
        assert!(set_node_type(&mut path, node_id, NodeType::Smooth));
        let editable = EditablePath::from_vecpath(&path);
        assert_eq!(editable[0].nodes[1].node_type, NodeType::Smooth);
        assert!(editable[0].nodes[1].handle_in.is_some());
        assert!(editable[0].nodes[1].handle_out.is_some());
        assert_eq!(editable[0].nodes[0].node_type, NodeType::Corner);
        assert_eq!(editable[0].nodes[2].node_type, NodeType::Corner);
        assert!(editable[0].nodes[0].handle_out.is_some());
        assert!(editable[0].nodes[2].handle_in.is_some());

        match path.subpaths[0].commands[1] {
            PathCommand::CubicTo { c1x, c1y, .. } => {
                assert!(
                    (c1x - 10.0).abs() < 1e-9 && (c1y - (10.0 / 3.0)).abs() < 1e-9,
                    "incoming segment should keep a normal tangent at the previous corner"
                );
            }
            _ => panic!("Expected materialized incoming cubic"),
        }
        match path.subpaths[0].commands[2] {
            PathCommand::CubicTo { c2x, c2y, .. } => {
                assert!(
                    (c2x - 50.0).abs() < 1e-9 && (c2y - (10.0 / 3.0)).abs() < 1e-9,
                    "outgoing segment should keep a normal tangent at the next corner"
                );
            }
            _ => panic!("Expected materialized outgoing cubic"),
        }
    }

    #[test]
    fn set_node_type_smooth_on_moveto_materializes_closing_segment() {
        let mut path = VecPath::parse_svg_d("M0 0 L30 0 L30 30 L0 30 Z");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 0,
        };
        assert!(set_node_type(&mut path, node_id, NodeType::Smooth));

        let commands = &path.subpaths[0].commands;
        assert!(matches!(commands[1], PathCommand::CubicTo { .. }));
        assert!(
            matches!(commands[4], PathCommand::CubicTo { x, y, .. } if x.abs() < 1e-9 && y.abs() < 1e-9)
        );
        assert!(matches!(commands[5], PathCommand::Close));

        let editable = EditablePath::from_vecpath(&path);
        assert_eq!(editable[0].nodes.len(), 4);
        assert_eq!(editable[0].nodes[0].node_type, NodeType::Smooth);
        assert!(editable[0].nodes[0].handle_in.is_some());
        assert!(editable[0].nodes[0].handle_out.is_some());
        assert_eq!(editable[0].nodes[1].node_type, NodeType::Corner);
        assert_eq!(editable[0].nodes[3].node_type, NodeType::Corner);
        assert!(editable[0].nodes[1].handle_in.is_some());
        assert!(editable[0].nodes[3].handle_out.is_some());
    }

    // --- convert_segment_to_line / convert_segment_to_curve tests ---

    #[test]
    fn convert_cubic_to_line() {
        let mut path = VecPath::parse_svg_d("M0 0 C10 20 30 20 40 0");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 1,
        };
        assert!(convert_segment_to_line(&mut path, node_id));
        assert!(matches!(
            path.subpaths[0].commands[1],
            PathCommand::LineTo { x, y } if (x - 40.0).abs() < 1e-9 && y.abs() < 1e-9
        ));
    }

    #[test]
    fn convert_line_to_line_is_noop() {
        let mut path = VecPath::parse_svg_d("M0 0 L10 10");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 1,
        };
        assert!(!convert_segment_to_line(&mut path, node_id));
    }

    #[test]
    fn convert_line_to_curve() {
        let mut path = VecPath::parse_svg_d("M0 0 L30 0");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 1,
        };
        assert!(convert_segment_to_curve(&mut path, node_id));
        match path.subpaths[0].commands[1] {
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                assert!((x - 30.0).abs() < 1e-9);
                assert!(y.abs() < 1e-9);
                assert!((c1x - 10.0).abs() < 1e-9);
                assert!(c1y.abs() < 1e-9);
                assert!((c2x - 20.0).abs() < 1e-9);
                assert!(c2y.abs() < 1e-9);
            }
            _ => panic!("Expected CubicTo"),
        }
    }

    #[test]
    fn convert_curve_to_curve_is_noop() {
        let mut path = VecPath::parse_svg_d("M0 0 C10 20 30 20 40 0");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 1,
        };
        assert!(!convert_segment_to_curve(&mut path, node_id));
    }

    // --- delete_segment tests ---

    #[test]
    fn delete_segment_opens_closed_path_preserving_nodes() {
        // Triangle M0,0 L10,0 L5,10 Z — delete edge B→C (cmd 2)
        // Expected: open path [M5,10, L0,0, L10,0] — all 3 nodes preserved
        let mut path = VecPath::parse_svg_d("M0 0 L10 0 L5 10 Z");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 2,
        };
        assert!(delete_segment(&mut path, node_id));
        assert!(!path.subpaths[0].closed);
        assert_eq!(path.subpaths[0].commands.len(), 3); // M, L, L — all nodes preserved
        // Starts at the deleted edge's destination
        assert!(matches!(
            path.subpaths[0].commands[0],
            PathCommand::MoveTo { x, y } if (x - 5.0).abs() < 1e-9 && (y - 10.0).abs() < 1e-9
        ));
    }

    #[test]
    fn delete_closing_segment_opens_path() {
        // Rectangle M0,0 L10,0 L10,10 L0,10 Z — delete the closing segment (Close at cmd 4)
        // Expected: open path with all 4 nodes
        let mut path = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        assert_eq!(path.subpaths[0].commands.len(), 5); // M, L, L, L, Z
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 4, // Close command
        };
        assert!(delete_segment(&mut path, node_id));
        assert!(!path.subpaths[0].closed);
        assert_eq!(path.subpaths[0].commands.len(), 4); // M, L, L, L — Close removed
    }

    #[test]
    fn delete_segment_splits_open_path() {
        let mut path = VecPath::parse_svg_d("M0 0 L10 0 L20 0 L30 0");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 2,
        };
        assert!(delete_segment(&mut path, node_id));
        assert_eq!(path.subpaths.len(), 2);
        assert_eq!(path.subpaths[0].commands.len(), 2);
        assert_eq!(path.subpaths[1].commands.len(), 2);
    }

    #[test]
    fn delete_segment_rejects_moveto() {
        let mut path = VecPath::parse_svg_d("M0 0 L10 10");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 0,
        };
        assert!(!delete_segment(&mut path, node_id));
    }

    // --- break_path_at_node tests ---

    #[test]
    fn break_at_node_opens_closed_path() {
        // Triangle M0,0 L10,0 L5,10 Z — break at L10,0 (cmd 1)
        // Expected: open path [M10,0, L5,10, L0,0, L10,0] — same outline, seam at break node
        let mut path = VecPath::parse_svg_d("M0 0 L10 0 L5 10 Z");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 1,
        };
        assert!(break_path_at_node(&mut path, node_id));
        assert!(!path.subpaths[0].closed);
        assert_eq!(path.subpaths[0].commands.len(), 4); // M, L, L, L — no segment removed
        assert!(matches!(
            path.subpaths[0].commands[0],
            PathCommand::MoveTo { x, y } if (x - 10.0).abs() < 1e-9 && y.abs() < 1e-9
        ));
        // Last command should end back at the break node so the visual outline is preserved.
        assert!(matches!(
            path.subpaths[0].commands[3],
            PathCommand::LineTo { x, y } if (x - 10.0).abs() < 1e-9 && y.abs() < 1e-9
        ));
    }

    #[test]
    fn break_at_node_differs_from_delete_segment_on_closed_path() {
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 1,
        };
        let mut broken = VecPath::parse_svg_d("M0 0 L10 0 L5 10 Z");
        let mut deleted = broken.clone();

        assert!(break_path_at_node(&mut broken, node_id));
        assert!(delete_segment(&mut deleted, node_id));

        assert_eq!(broken.subpaths[0].commands.len(), 4);
        assert_eq!(deleted.subpaths[0].commands.len(), 3);
        assert!(matches!(
            broken.subpaths[0].commands.last(),
            Some(PathCommand::LineTo { x, y }) if (*x - 10.0).abs() < 1e-9 && y.abs() < 1e-9
        ));
        assert!(matches!(
            deleted.subpaths[0].commands.last(),
            Some(PathCommand::LineTo { x, y }) if x.abs() < 1e-9 && y.abs() < 1e-9
        ));
    }

    #[test]
    fn break_at_node_splits_open_path() {
        let mut path = VecPath::parse_svg_d("M0 0 L10 0 L20 0 L30 0");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 2,
        };
        assert!(break_path_at_node(&mut path, node_id));
        assert_eq!(path.subpaths.len(), 2);
        let last_cmd = path.subpaths[0].commands.last().unwrap();
        assert!(matches!(
            last_cmd,
            PathCommand::LineTo { x, y } if (x - 20.0).abs() < 1e-9 && y.abs() < 1e-9
        ));
        assert!(matches!(
            path.subpaths[1].commands[0],
            PathCommand::MoveTo { x, y } if (x - 20.0).abs() < 1e-9 && y.abs() < 1e-9
        ));
    }

    #[test]
    fn break_at_moveto_opens_closed() {
        let mut path = VecPath::parse_svg_d("M0 0 L10 10 Z");
        let node_id = NodeId {
            subpath_idx: 0,
            command_idx: 0,
        };
        assert!(break_path_at_node(&mut path, node_id));
        assert!(!path.subpaths[0].closed);
        assert!(matches!(
            path.subpaths[0].commands.last(),
            Some(PathCommand::LineTo { x, y }) if x.abs() < 1e-9 && y.abs() < 1e-9
        ));
    }

    // --- toggle_path_closed tests ---

    #[test]
    fn toggle_open_to_closed() {
        let mut path = VecPath::parse_svg_d("M0 0 L10 0 L10 10");
        assert!(toggle_path_closed(&mut path, 0));
        assert!(path.subpaths[0].closed);
        assert!(matches!(
            path.subpaths[0].commands.last(),
            Some(PathCommand::Close)
        ));
    }

    #[test]
    fn toggle_closed_to_open() {
        let mut path = VecPath::parse_svg_d("M0 0 L10 0 L10 10 Z");
        assert!(toggle_path_closed(&mut path, 0));
        assert!(!path.subpaths[0].closed);
        assert!(
            !path.subpaths[0]
                .commands
                .iter()
                .any(|c| matches!(c, PathCommand::Close))
        );
    }

    #[test]
    fn toggle_invalid_subpath_index() {
        let mut path = VecPath::parse_svg_d("M0 0 L10 10");
        assert!(!toggle_path_closed(&mut path, 5));
    }
}

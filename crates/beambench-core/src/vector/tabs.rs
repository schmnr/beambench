use beambench_common::Point2D;
use beambench_common::path::{PathCommand, SubPath, VecPath};
use serde::{Deserialize, Serialize};

use crate::object::TabAnchor;
use crate::vector::flatten::{DEFAULT_TOLERANCE_MM, flatten_vecpath};
use crate::vector::trim::project_point_on_polyline_with_dist;

/// Add tabs (gaps) to a closed path at evenly-spaced positions.
pub fn add_tabs(path: &VecPath, count: u32, width_mm: f64) -> VecPath {
    if count == 0 || width_mm <= 0.0 {
        return path.clone();
    }

    let polylines = flatten_vecpath(path, DEFAULT_TOLERANCE_MM);

    let mut result_subpaths = Vec::new();

    for polyline in polylines {
        if !polyline.closed || polyline.points.len() < 3 {
            // Can't add tabs to open paths
            let commands = points_to_commands(&polyline.points, polyline.closed);
            result_subpaths.push(SubPath {
                commands,
                closed: polyline.closed,
            });
            continue;
        }

        let total_len = compute_perimeter(&polyline.points);
        let tab_spacing = total_len / count as f64;

        let mut segments: Vec<Vec<Point2D>> = Vec::new();
        let mut current_segment: Vec<Point2D> = Vec::new();

        let mut current_len = 0.0;
        let mut next_tab = tab_spacing / 2.0; // Start with offset
        let mut in_gap = false;
        let mut gap_end = 0.0;

        let n = polyline.points.len();
        for i in 0..n {
            let p1 = polyline.points[i];
            let p2 = polyline.points[(i + 1) % n];

            let dx = p2.x - p1.x;
            let dy = p2.y - p1.y;
            let seg_len = (dx * dx + dy * dy).sqrt();
            let seg_start = current_len;
            let seg_end = current_len + seg_len;

            if seg_len <= f64::EPSILON {
                // Zero-length segment: nothing to walk.
                if !in_gap {
                    push_point(&mut current_segment, p1);
                }
                current_len = seg_end;
                continue;
            }

            if !in_gap {
                push_point(&mut current_segment, p1);
            }

            // A single segment may contain several events (a gap closing,
            // one or more tabs starting), so keep processing events until
            // none remain within this segment.
            loop {
                if in_gap {
                    if gap_end <= seg_end {
                        // Gap closes within this segment: start the next cut
                        // segment at the gap-end point.
                        let t = ((gap_end - seg_start) / seg_len).clamp(0.0, 1.0);
                        push_point(
                            &mut current_segment,
                            Point2D {
                                x: p1.x + dx * t,
                                y: p1.y + dy * t,
                            },
                        );
                        in_gap = false;
                        // Keep tab positions monotonic even if width >= spacing
                        // (overlapping gaps degenerate case).
                        next_tab = (next_tab + tab_spacing).max(gap_end);
                    } else {
                        // Gap consumes the rest of this segment (and possibly
                        // following segments too).
                        break;
                    }
                } else if next_tab <= seg_end {
                    // A tab starts within this segment: emit the partial
                    // segment up to the gap start and close out the cut.
                    let t = ((next_tab - seg_start) / seg_len).clamp(0.0, 1.0);
                    push_point(
                        &mut current_segment,
                        Point2D {
                            x: p1.x + dx * t,
                            y: p1.y + dy * t,
                        },
                    );
                    if current_segment.len() >= 2 {
                        segments.push(std::mem::take(&mut current_segment));
                    } else {
                        // Tab starts exactly at the cut start: drop the
                        // zero-length point instead of emitting it.
                        current_segment.clear();
                    }
                    in_gap = true;
                    gap_end = next_tab + width_mm;
                } else {
                    // No more events in this segment.
                    break;
                }
            }

            current_len = seg_end;
        }

        // Close the walk back at the start point and flush the remaining
        // segment. A gap extending past the end of the polyline is clamped:
        // the walk simply ends inside the gap.
        if !in_gap {
            push_point(&mut current_segment, polyline.points[0]);
        }
        if current_segment.len() >= 2 {
            segments.push(current_segment);
        }

        // Convert segments to subpaths
        for segment in segments {
            if segment.len() >= 2 {
                let commands = points_to_commands(&segment, false);
                result_subpaths.push(SubPath {
                    commands,
                    closed: false,
                });
            }
        }
    }

    VecPath {
        subpaths: result_subpaths,
    }
}

/// Resolved tab marker with world-space coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabMarkerResolved {
    pub subpath_index: usize,
    pub position: f64,
    pub world_x: f64,
    pub world_y: f64,
    /// Index into the original tabs array, so callers can map back correctly
    /// even when some anchors fail to resolve (e.g. stale subpath_index).
    pub original_index: usize,
}

/// Project a click point onto a VecPath, returning the best TabAnchor and distance.
///
/// Only considers closed subpaths. Returns `None` if the path has no closed subpaths.
pub fn project_point_to_tab_anchor(path: &VecPath, click: Point2D) -> Option<(TabAnchor, f64)> {
    let polylines = flatten_vecpath(path, DEFAULT_TOLERANCE_MM);

    let mut best: Option<(TabAnchor, f64)> = None;

    for (sp_idx, polyline) in polylines.iter().enumerate() {
        if !polyline.closed || polyline.points.len() < 3 {
            continue;
        }

        let (dist, arc_len) = project_point_on_polyline_with_dist(click, &polyline.points, true);
        let perimeter = compute_perimeter(&polyline.points);
        if perimeter < f64::EPSILON {
            continue;
        }
        let position = (arc_len / perimeter).clamp(0.0, 1.0);

        if best.is_none() || dist < best.as_ref().unwrap().1 {
            best = Some((
                TabAnchor {
                    subpath_index: sp_idx,
                    position,
                },
                dist,
            ));
        }
    }

    best
}

/// Convert a TabAnchor back to a world-space point on the path.
pub fn position_to_world_point(path: &VecPath, anchor: &TabAnchor) -> Option<Point2D> {
    let polylines = flatten_vecpath(path, DEFAULT_TOLERANCE_MM);
    let polyline = polylines.get(anchor.subpath_index)?;

    if !polyline.closed || polyline.points.len() < 3 {
        return None;
    }

    let perimeter = compute_perimeter(&polyline.points);
    if perimeter < f64::EPSILON {
        return None;
    }

    let target_len = anchor.position.clamp(0.0, 1.0) * perimeter;
    let mut cum = 0.0;

    let n = polyline.points.len();
    for i in 0..n {
        let a = polyline.points[i];
        let b = polyline.points[(i + 1) % n];
        let seg_len = a.distance_to(&b);

        if cum + seg_len >= target_len - f64::EPSILON {
            let t = if seg_len > f64::EPSILON {
                ((target_len - cum) / seg_len).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return Some(Point2D::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
        }

        cum += seg_len;
    }

    // Fallback: return last point
    polyline.points.last().copied()
}

/// Resolve all tab anchors to world-space coordinates.
///
/// Each resolved marker carries its `original_index` into the source `tabs` slice,
/// so callers can map back correctly even when some anchors fail to resolve.
pub fn resolve_tab_positions(path: &VecPath, tabs: &[TabAnchor]) -> Vec<TabMarkerResolved> {
    tabs.iter()
        .enumerate()
        .filter_map(|(idx, anchor)| {
            position_to_world_point(path, anchor).map(|pt| TabMarkerResolved {
                subpath_index: anchor.subpath_index,
                position: anchor.position,
                world_x: pt.x,
                world_y: pt.y,
                original_index: idx,
            })
        })
        .collect()
}

/// Push a point onto a cut segment, skipping duplicates of the last point.
/// Duplicates arise when a gap boundary lands exactly on a vertex.
fn push_point(segment: &mut Vec<Point2D>, pt: Point2D) {
    if let Some(last) = segment.last()
        && (last.x - pt.x).abs() < 1e-9
        && (last.y - pt.y).abs() < 1e-9
    {
        return;
    }
    segment.push(pt);
}

fn compute_perimeter(points: &[Point2D]) -> f64 {
    let mut total = 0.0;
    for i in 0..points.len() {
        let p1 = points[i];
        let p2 = points[(i + 1) % points.len()];
        let dx = p2.x - p1.x;
        let dy = p2.y - p1.y;
        total += (dx * dx + dy * dy).sqrt();
    }
    total
}

fn points_to_commands(points: &[Point2D], closed: bool) -> Vec<PathCommand> {
    if points.is_empty() {
        return vec![];
    }

    let mut commands = vec![PathCommand::MoveTo {
        x: points[0].x,
        y: points[0].y,
    }];

    for pt in &points[1..] {
        commands.push(PathCommand::LineTo { x: pt.x, y: pt.y });
    }

    if closed {
        commands.push(PathCommand::Close);
    }

    commands
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::ShapeKind;
    use crate::vector::convert::shape_to_vecpath;

    /// Build a closed polygon VecPath from a list of vertices.
    fn polygon(points: &[(f64, f64)]) -> VecPath {
        let mut commands = vec![PathCommand::MoveTo {
            x: points[0].0,
            y: points[0].1,
        }];
        for &(x, y) in &points[1..] {
            commands.push(PathCommand::LineTo { x, y });
        }
        commands.push(PathCommand::Close);
        VecPath {
            subpaths: vec![SubPath {
                commands,
                closed: true,
            }],
        }
    }

    /// Total drawn length of all (open) subpaths in a result path.
    fn output_length(path: &VecPath) -> f64 {
        flatten_vecpath(path, DEFAULT_TOLERANCE_MM)
            .iter()
            .map(|pl| {
                pl.points
                    .windows(2)
                    .map(|w| w[0].distance_to(&w[1]))
                    .sum::<f64>()
            })
            .sum()
    }

    #[test]
    fn add_tabs_to_rectangle() {
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 100.0, 100.0, 0.0);
        let result = add_tabs(&rect, 4, 5.0);

        assert!(!result.is_empty());
        // Should create multiple subpaths (segments between tabs)
        assert!(result.subpaths.len() >= 4);
    }

    #[test]
    fn add_tabs_zero_count() {
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 100.0, 100.0, 0.0);
        let result = add_tabs(&rect, 0, 5.0);

        // Should return original path
        assert!(!result.is_empty());
    }

    #[test]
    fn add_tabs_to_circle() {
        let circle = shape_to_vecpath(ShapeKind::Ellipse, 50.0, 50.0, 0.0);
        let result = add_tabs(&circle, 6, 3.0);

        assert!(!result.is_empty());
    }

    #[test]
    fn add_tabs_zero_width() {
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 100.0, 100.0, 0.0);
        let result = add_tabs(&rect, 4, 0.0);

        // Zero width should return original
        assert!(!result.is_empty());
    }

    #[test]
    fn tab_gap_spanning_corner_is_cut() {
        // Right triangle (0,0) -> (10,0) -> (10,10), perimeter = 20 + sqrt(200).
        // One tab at perimeter/2 ~= 17.07 with width 6: the gap runs from
        // ~17.07 to ~23.07, straddling the corner at (10,10) (arc length 20).
        // The old condition required the whole gap inside one segment, so the
        // gap was never cut at all.
        let tri = polygon(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]);
        let perimeter = 20.0 + 200.0_f64.sqrt();

        let result = add_tabs(&tri, 1, 6.0);

        // A gap must actually exist: drawn length = perimeter - 1 tab * 6mm.
        let len = output_length(&result);
        assert!(
            (len - (perimeter - 6.0)).abs() < 1e-6,
            "Expected drawn length {} but got {}",
            perimeter - 6.0,
            len
        );
        // One gap mid-walk splits the closed path into two open pieces.
        assert_eq!(result.subpaths.len(), 2);
    }

    #[test]
    fn tab_gap_spanning_multiple_short_segments() {
        // Rectangle outline with several 1mm collinear segments on the bottom
        // edge. Perimeter = 60. Two tabs (spacing 30) at arc 15 and 45, width
        // 6: the first gap (15..21) consumes the remainder of one short
        // segment, two more whole segments, and part of the right edge; the
        // second gap (45..51) straddles the (0,10) corner.
        let path = polygon(&[
            (0.0, 0.0),
            (12.5, 0.0),
            (13.5, 0.0),
            (14.5, 0.0),
            (15.5, 0.0),
            (16.5, 0.0),
            (20.0, 0.0),
            (20.0, 10.0),
            (0.0, 10.0),
        ]);

        let result = add_tabs(&path, 2, 6.0);

        let len = output_length(&result);
        assert!(
            (len - 48.0).abs() < 1e-6,
            "Expected drawn length 48 (60 - 2*6) but got {len}"
        );
        // Two gaps mid-walk -> three open pieces.
        assert_eq!(result.subpaths.len(), 3);
    }

    #[test]
    fn two_tabs_within_one_long_segment() {
        // Long thin rectangle 100x5, perimeter = 210. Four tabs (spacing
        // 52.5) at arc 26.25, 78.75, 131.25, 183.75 with width 4: the bottom
        // edge (arc 0..100) contains two full gaps, and the top edge (arc
        // 105..205) contains the other two.
        let rect = polygon(&[(0.0, 0.0), (100.0, 0.0), (100.0, 5.0), (0.0, 5.0)]);

        let result = add_tabs(&rect, 4, 4.0);

        let len = output_length(&result);
        assert!(
            (len - 194.0).abs() < 1e-6,
            "Expected drawn length 194 (210 - 4*4) but got {len}"
        );
        // Four gaps mid-walk -> five open pieces.
        assert_eq!(result.subpaths.len(), 5);
    }

    #[test]
    fn tab_starting_exactly_on_vertex() {
        // Square 10x10, perimeter = 40. One tab at arc 20 with width 4: the
        // gap starts exactly on the (10,10) corner and ends inside the top
        // edge at (6,10).
        let square = polygon(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);

        let result = add_tabs(&square, 1, 4.0);

        let len = output_length(&result);
        assert!(
            (len - 36.0).abs() < 1e-6,
            "Expected drawn length 36 (40 - 4) but got {len}"
        );
        assert_eq!(result.subpaths.len(), 2);

        // The first piece must end exactly at the corner vertex (no
        // zero-length point, no dropped corner).
        let polylines = flatten_vecpath(&result, DEFAULT_TOLERANCE_MM);
        let first_end = *polylines[0].points.last().unwrap();
        assert!(
            (first_end.x - 10.0).abs() < 1e-9 && (first_end.y - 10.0).abs() < 1e-9,
            "First piece should end at the (10,10) corner, got ({}, {})",
            first_end.x,
            first_end.y
        );
        // The second piece starts at the gap end (6,10).
        let second_start = polylines[1].points[0];
        assert!(
            (second_start.x - 6.0).abs() < 1e-9 && (second_start.y - 10.0).abs() < 1e-9,
            "Second piece should start at (6,10), got ({}, {})",
            second_start.x,
            second_start.y
        );
    }

    #[test]
    fn project_point_to_tab_anchor_on_rectangle() {
        // Rectangle from (0,0) to (100,100) — perimeter = 400
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 100.0, 100.0, 0.0);
        // Click near the midpoint of the bottom edge (50, 0)
        let click = Point2D::new(50.0, 0.0);
        let (anchor, dist) = project_point_to_tab_anchor(&rect, click).unwrap();

        assert_eq!(anchor.subpath_index, 0);
        // Bottom edge midpoint is at ~50/400 = 0.125 along the perimeter
        assert!(anchor.position >= 0.0 && anchor.position <= 1.0);
        assert!(dist < 1.0, "Distance should be very small: {dist}");
    }

    #[test]
    fn position_to_world_point_roundtrip() {
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 100.0, 100.0, 0.0);
        // Place a tab at 0.25 (should be at the corner/midpoint of second edge)
        let anchor = TabAnchor {
            subpath_index: 0,
            position: 0.25,
        };
        let pt = position_to_world_point(&rect, &anchor).unwrap();
        // At 0.25 of perimeter 400 = 100mm, which is the end of first edge
        // For a rect (0,0)-(100,100), going CW: at 100mm we should be at (100,0) or the next corner
        assert!(
            pt.x >= -1.0 && pt.x <= 101.0 && pt.y >= -1.0 && pt.y <= 101.0,
            "Point should be on the rectangle edge: ({}, {})",
            pt.x,
            pt.y
        );
    }

    #[test]
    fn resolve_tab_positions_returns_world_coordinates() {
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 100.0, 100.0, 0.0);
        let tabs = vec![
            TabAnchor {
                subpath_index: 0,
                position: 0.0,
            },
            TabAnchor {
                subpath_index: 0,
                position: 0.5,
            },
        ];
        let resolved = resolve_tab_positions(&rect, &tabs);
        assert_eq!(resolved.len(), 2);
        // Both should have valid world coordinates on the rectangle
        for m in &resolved {
            assert!(
                m.world_x >= -1.0 && m.world_x <= 101.0 && m.world_y >= -1.0 && m.world_y <= 101.0,
                "Resolved point should be on rectangle: ({}, {})",
                m.world_x,
                m.world_y
            );
        }
    }

    #[test]
    fn project_returns_none_for_open_only_path() {
        // Create an open path
        let open_path = VecPath {
            subpaths: vec![SubPath {
                commands: vec![
                    PathCommand::MoveTo { x: 0.0, y: 0.0 },
                    PathCommand::LineTo { x: 100.0, y: 0.0 },
                ],
                closed: false,
            }],
        };
        let result = project_point_to_tab_anchor(&open_path, Point2D::new(50.0, 0.0));
        assert!(result.is_none(), "Should return None for open-only path");
    }

    #[test]
    fn multi_contour_tabs_anchor_to_correct_subpath() {
        // Two nested rectangles as separate subpaths
        let path = VecPath {
            subpaths: vec![
                SubPath {
                    commands: vec![
                        PathCommand::MoveTo { x: 0.0, y: 0.0 },
                        PathCommand::LineTo { x: 100.0, y: 0.0 },
                        PathCommand::LineTo { x: 100.0, y: 100.0 },
                        PathCommand::LineTo { x: 0.0, y: 100.0 },
                        PathCommand::Close,
                    ],
                    closed: true,
                },
                SubPath {
                    commands: vec![
                        PathCommand::MoveTo { x: 20.0, y: 20.0 },
                        PathCommand::LineTo { x: 80.0, y: 20.0 },
                        PathCommand::LineTo { x: 80.0, y: 80.0 },
                        PathCommand::LineTo { x: 20.0, y: 80.0 },
                        PathCommand::Close,
                    ],
                    closed: true,
                },
            ],
        };

        // Click near the inner rectangle's top edge
        let click = Point2D::new(50.0, 20.0);
        let (anchor, dist) = project_point_to_tab_anchor(&path, click).unwrap();
        // Should snap to the inner rectangle (subpath_index 1) since it's closer
        assert_eq!(anchor.subpath_index, 1, "Should anchor to inner rectangle");
        assert!(dist < 1.0, "Should be very close to the edge");
    }

    #[test]
    fn tab_anchor_serialization_roundtrip() {
        let anchor = TabAnchor {
            subpath_index: 2,
            position: 0.75,
        };
        let json = serde_json::to_string(&anchor).unwrap();
        let restored: TabAnchor = serde_json::from_str(&json).unwrap();
        assert_eq!(anchor, restored);
    }
}

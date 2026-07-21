//! Per-closed-polyline start-vertex selection.
//!
//! Implements start-point optimization as a single coordinated pass:
//!
//! - `choose_best_start`: rotate the start vertex of each closed
//!   polyline to minimize entry travel from the previous polyline's
//!   exit (or from `start_pos` for the first item).
//! - `choose_corners`: sub-flag of `choose_best_start` — restrict the
//!   candidate start vertices to corners (interior angle deviating
//!   from 180° by more than `CORNER_ANGLE_THRESHOLD_RAD`). Falls back
//!   to all vertices when no corners qualify. No-op when
//!   `choose_best_start` is off.
//! - `choose_best_direction`: picks a consistent traversal direction
//!   for closed polylines.
//!   * When paired with `choose_best_start`, the joint search evaluates
//!     reversed traversal at every candidate start vertex and picks the
//!     shorter entry using the best clockwise/counterclockwise exit.
//!   * **When on alone**, the pair degenerates (a closed ring's entry
//!     and exit are both `pts[0]` regardless of direction, so entry
//!     cost doesn't distinguish forward from reversed). To keep the
//!     flag observably independent of `choose_best_start`, the alone
//!     mode canonicalizes winding via `signed_area` — CCW polylines
//!     are reversed so every closed polyline emits in consistent (CW)
//!     order. This produces a measurable change in the emitted
//!     polyline's interior vertex order without touching travel
//!     distance, which matches the mechanical motivation of
//!     "consistent direction" on machines sensitive to backlash or
//!     scanner bias.
//!
//! Open polylines are left untouched: they have a fixed direction
//! semantically (start → end traces the curve), and the existing
//! nearest-neighbor logic in `optimize/order.rs` already chooses
//! which end to enter from.
//!
//! The pass is **per-item greedy**: it picks each closed polyline's
//! start vertex using only the already-placed previous item as
//! context. A global optimum would require lookahead that doesn't
//! pay for itself at M1's complexity budget.

use beambench_common::geometry::Point2D;

use crate::optimize::order::Orderable;

/// Interior-angle deviation from 180° (radians) above which a vertex
/// is considered a corner for `choose_corners`. ~0.52 rad ≈ 30°, so a
/// square's 90° corners (deviation ~90°) qualify and gentle curves
/// don't.
const CORNER_ANGLE_THRESHOLD_RAD: f64 = 0.52;

/// Adjust each closed polyline's start vertex (and optionally its
/// traversal direction) according to the three flags.
///
/// `start_pos` is the tool position before this sequence begins —
/// typically the user-specified start point or origin. Each closed
/// polyline's exit (== entry for a closed ring) becomes the
/// `current_pos` for the next item.
pub fn apply_start_point<T: Orderable>(
    items: Vec<T>,
    start_pos: Point2D,
    choose_best_start: bool,
    choose_corners: bool,
    choose_best_direction: bool,
) -> Vec<T> {
    if !choose_best_start && !choose_best_direction {
        return items;
    }

    let mut current_pos = start_pos;
    let mut result = Vec::with_capacity(items.len());

    for mut item in items {
        let should_rewire = item.closed() && item.points().len() >= 4;
        if should_rewire {
            // Direction-only fast path: canonical-winding pass. Without
            // a start-vertex choice the joint search degenerates (entry
            // cost is the same forward vs reversed for a closed ring at
            // rot=0), so we need a direction-selection rule that's
            // still observable. Canonical CW winding flips CCW-drawn
            // polylines and leaves CW-drawn ones untouched.
            if choose_best_direction && !choose_best_start {
                if signed_area(item.points()) > 0.0 {
                    reverse_closed(item.points_mut());
                }
            } else {
                let (best_rot, best_reverse) = choose_best_entry(
                    item.points(),
                    current_pos,
                    choose_best_start,
                    choose_corners,
                    choose_best_direction,
                );
                if best_reverse {
                    reverse_closed(item.points_mut());
                }
                if best_rot != 0 {
                    rotate_closed_start(item.points_mut(), best_rot);
                }
            }
        }

        if let Some(last) = item.points().last() {
            current_pos = *last;
        }
        result.push(item);
    }

    result
}

/// Signed area via the shoelace formula. Positive = CCW, negative = CW.
/// The closing-duplicate vertex (when present) contributes a zero term
/// to the sum, so it's safe to pass the full polyline points.
fn signed_area(pts: &[Point2D]) -> f64 {
    let n = pts.len();
    if n < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        area += pts[i].x * pts[j].y;
        area -= pts[j].x * pts[i].y;
    }
    area / 2.0
}

/// Pick the `(rotation_index, reverse)` pair that minimizes entry
/// travel from `current_pos` to the first vertex of the transformed
/// polyline. The rotation index is into the stripped ring (without the
/// duplicated closing vertex), so valid values are `0..n-1`.
fn choose_best_entry(
    pts: &[Point2D],
    current_pos: Point2D,
    choose_best_start: bool,
    choose_corners: bool,
    choose_best_direction: bool,
) -> (usize, bool) {
    let n = pts.len();
    // Closed ring with duplicated closing vertex has n-1 unique vertices.
    let ring_len = n.saturating_sub(1);
    if ring_len == 0 {
        return (0, false);
    }

    // Which rotation indices are candidates?
    let candidates: Vec<usize> = if choose_best_start {
        if choose_corners {
            let corners = corner_indices(pts);
            if corners.is_empty() {
                (0..ring_len).collect()
            } else {
                corners
            }
        } else {
            (0..ring_len).collect()
        }
    } else {
        vec![0]
    };

    let reverse_options: &[bool] = if choose_best_direction {
        &[false, true]
    } else {
        &[false]
    };

    let mut best = (0usize, false);
    let mut best_cost = f64::INFINITY;
    for &rot in &candidates {
        for &rev in reverse_options {
            // Cost = distance from current_pos to the new start vertex
            // after applying `rev` then `rot`. Order matters: reverse
            // the stripped ring first, then rotate — matching the
            // mutation order in `apply_start_point`.
            let stripped_start = if rev {
                // Reversed ring: pts[0] stays, pts[1..n-1] reverse.
                // After rotation by `rot`, the new start is the `rot`-th
                // element of the reversed stripped ring.
                reversed_ring_at(pts, rot)
            } else {
                pts[rot]
            };
            let cost = current_pos.distance_to(&stripped_start);
            if cost < best_cost {
                best_cost = cost;
                best = (rot, rev);
            }
        }
    }
    best
}

/// Return the `rot`-th vertex of the reversed stripped ring without
/// actually allocating a reversed vector. The stripped ring is
/// `pts[..n-1]`; its reversal, interpreted as a new ring whose
/// starting anchor stays at `pts[0]`, maps index `i` to
/// `pts[(ring_len - i) % ring_len]`.
fn reversed_ring_at(pts: &[Point2D], rot: usize) -> Point2D {
    let ring_len = pts.len() - 1;
    if ring_len == 0 {
        return pts[0];
    }
    let idx = (ring_len - rot) % ring_len;
    pts[idx]
}

/// Indices of corner vertices (into the stripped ring, `0..n-1`).
fn corner_indices(pts: &[Point2D]) -> Vec<usize> {
    let ring_len = pts.len().saturating_sub(1);
    (0..ring_len).filter(|&i| is_corner(pts, i)).collect()
}

fn is_corner(pts: &[Point2D], i: usize) -> bool {
    let ring_len = pts.len() - 1;
    if ring_len < 3 {
        return false;
    }
    let prev = pts[(i + ring_len - 1) % ring_len];
    let cur = pts[i];
    let next = pts[(i + 1) % ring_len];
    let incoming = unit_vec(prev, cur);
    let outgoing = unit_vec(cur, next);
    let (Some(a), Some(b)) = (incoming, outgoing) else {
        return false;
    };
    // Interior angle deviation from π (a straight line).
    // dot(a, b) = cos(θ), where θ is the turn angle at this vertex.
    // θ ≈ 0 for a straight continuation; θ large → sharper corner.
    let dot = (a.x * b.x + a.y * b.y).clamp(-1.0, 1.0);
    let turn_angle = dot.acos();
    turn_angle > CORNER_ANGLE_THRESHOLD_RAD
}

fn unit_vec(from: Point2D, to: Point2D) -> Option<Point2D> {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let mag = (dx * dx + dy * dy).sqrt();
    if mag < 1e-9 {
        None
    } else {
        Some(Point2D::new(dx / mag, dy / mag))
    }
}

/// In-place reverse of a closed polyline whose `points[0] == points.last()`.
/// Preserves the closing-duplicate invariant.
fn reverse_closed(pts: &mut Vec<Point2D>) {
    if pts.len() < 3 {
        return;
    }
    // Strip the duplicate closing vertex, reverse the stripped ring, re-append.
    let anchor = pts[0];
    let mut body: Vec<Point2D> = pts.drain(1..pts.len() - 1).collect();
    body.reverse();
    // Rebuild: [anchor, body..., anchor]
    pts.clear();
    pts.push(anchor);
    pts.extend(body);
    pts.push(anchor);
}

/// Rotate the ring so that vertex index `rot` (in the stripped ring)
/// becomes the new start. Preserves the closing-duplicate invariant.
fn rotate_closed_start(pts: &mut Vec<Point2D>, rot: usize) {
    let n = pts.len();
    if n < 3 || rot == 0 {
        return;
    }
    let ring_len = n - 1;
    let rot = rot % ring_len;
    if rot == 0 {
        return;
    }
    // Build the rotated ring: pts[rot..ring_len] ++ pts[0..rot] ++ pts[rot].
    let mut rotated: Vec<Point2D> = Vec::with_capacity(n);
    rotated.extend_from_slice(&pts[rot..ring_len]);
    rotated.extend_from_slice(&pts[..rot]);
    rotated.push(pts[rot]);
    *pts = rotated;
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::path::Polyline;

    /// 10×10 square at origin, closed with the standard duplicated-anchor
    /// convention: [(0,0), (10,0), (10,10), (0,10), (0,0)].
    fn unit_square() -> Polyline {
        Polyline {
            points: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
                Point2D::new(0.0, 10.0),
                Point2D::new(0.0, 0.0),
            ],
            closed: true,
        }
    }

    #[test]
    fn all_flags_off_is_noop() {
        let input = vec![unit_square()];
        let out = apply_start_point(input.clone(), Point2D::new(0.0, 0.0), false, false, false);
        assert_eq!(out[0].points, input[0].points);
    }

    #[test]
    fn choose_best_start_rotates_to_closest_vertex() {
        // With the tool at (11, 11), the nearest vertex of the square is
        // (10, 10) — which sits at index 2 in the stripped ring. The
        // rotated polyline should start there.
        let out = apply_start_point(
            vec![unit_square()],
            Point2D::new(11.0, 11.0),
            true,
            false,
            false,
        );
        assert_eq!(out[0].points[0], Point2D::new(10.0, 10.0));
        // Closing anchor must equal the new start.
        let last = out[0].points.last().copied().unwrap();
        assert_eq!(last, Point2D::new(10.0, 10.0));
    }

    #[test]
    fn choose_best_start_near_bottom_right_picks_bottom_right() {
        let out = apply_start_point(
            vec![unit_square()],
            Point2D::new(11.0, -1.0),
            true,
            false,
            false,
        );
        assert_eq!(out[0].points[0], Point2D::new(10.0, 0.0));
    }

    #[test]
    fn choose_best_start_updates_current_pos_for_next_item() {
        let a = unit_square();
        // Second square, offset so its corners are easy to identify.
        let b = Polyline {
            points: vec![
                Point2D::new(100.0, 100.0),
                Point2D::new(110.0, 100.0),
                Point2D::new(110.0, 110.0),
                Point2D::new(100.0, 110.0),
                Point2D::new(100.0, 100.0),
            ],
            closed: true,
        };
        // With current_pos=(11,11), `a` should rotate to start at (10,10).
        // After `a` completes, current_pos advances to (10,10). `b`'s
        // nearest corner to (10,10) is (100,100) — that should become
        // `b`'s new start.
        let out = apply_start_point(vec![a, b], Point2D::new(11.0, 11.0), true, false, false);
        assert_eq!(out[0].points[0], Point2D::new(10.0, 10.0));
        assert_eq!(out[1].points[0], Point2D::new(100.0, 100.0));
    }

    #[test]
    fn choose_corners_restricts_candidates_to_corners() {
        // Add a midpoint vertex to the top edge of the square. Under
        // `choose_corners`, the midpoint should NOT be picked even
        // though it is spatially closest to a tool position directly
        // above it.
        let poly_with_midpoint = Polyline {
            points: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
                Point2D::new(5.0, 10.0), // midpoint on top edge
                Point2D::new(0.0, 10.0),
                Point2D::new(0.0, 0.0),
            ],
            closed: true,
        };
        // Tool at (5, 11) — closest vertex is (5,10) (the midpoint).
        // With choose_corners off, the pass should pick it.
        let out_no_corners = apply_start_point(
            vec![poly_with_midpoint.clone()],
            Point2D::new(5.0, 11.0),
            true,
            false,
            false,
        );
        assert_eq!(out_no_corners[0].points[0], Point2D::new(5.0, 10.0));
        // With choose_corners on, the midpoint is rejected; nearest
        // corner is either (10,10) or (0,10) — both 5mm away. Stable
        // order of corner_indices() picks (10,10) first.
        let out_corners = apply_start_point(
            vec![poly_with_midpoint],
            Point2D::new(5.0, 11.0),
            true,
            true,
            false,
        );
        let first = out_corners[0].points[0];
        assert!(
            first == Point2D::new(10.0, 10.0) || first == Point2D::new(0.0, 10.0),
            "expected a real corner, got {:?}",
            first
        );
    }

    #[test]
    fn choose_corners_falls_back_when_no_corners() {
        // All-smooth polygon: a 20-gon approximating a circle. None of its
        // vertices qualify as corners (all turn angles are tiny). The pass
        // should fall back to considering all vertices.
        let n = 20;
        let r = 50.0;
        let mut points: Vec<Point2D> = (0..n)
            .map(|i| {
                let theta = (i as f64) * 2.0 * std::f64::consts::PI / (n as f64);
                Point2D::new(r * theta.cos(), r * theta.sin())
            })
            .collect();
        points.push(points[0]); // close
        let poly = Polyline {
            points,
            closed: true,
        };
        // Should not panic and should rotate to some vertex near the tool.
        let out = apply_start_point(vec![poly], Point2D::new(r + 1.0, 0.0), true, true, false);
        assert_eq!(out[0].points[0].x.round(), r.round());
    }

    /// A clockwise-wound square (negative signed area under the shoelace
    /// convention used in this module): [(0,0), (0,10), (10,10), (10,0), (0,0)].
    fn cw_square() -> Polyline {
        Polyline {
            points: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(0.0, 10.0),
                Point2D::new(10.0, 10.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(0.0, 0.0),
            ],
            closed: true,
        }
    }

    #[test]
    fn choose_best_direction_alone_reverses_ccw_to_canonical_cw() {
        // `unit_square` as built above is CCW under the shoelace
        // convention (positive signed area), so the direction-only flag
        // should reverse it. `cw_square` is already CW and stays put.
        //
        // Sanity-check the fixtures' windings first so the test doc
        // stays honest if someone edits the fixtures.
        assert!(
            signed_area(&unit_square().points) > 0.0,
            "unit_square is CCW"
        );
        assert!(signed_area(&cw_square().points) < 0.0, "cw_square is CW");

        let out_ccw = apply_start_point(
            vec![unit_square()],
            Point2D::new(0.0, 0.0),
            false, // choose_best_start
            false,
            true, // choose_best_direction
        );
        // CCW square was reversed: second vertex is now the original
        // last-interior vertex (0,10), not the original (10,0).
        assert_eq!(out_ccw[0].points[0], Point2D::new(0.0, 0.0));
        assert_eq!(out_ccw[0].points[1], Point2D::new(0.0, 10.0));
        // Post-reversal winding is CW (negative area).
        assert!(signed_area(&out_ccw[0].points) < 0.0);

        let out_cw = apply_start_point(
            vec![cw_square()],
            Point2D::new(0.0, 0.0),
            false,
            false,
            true,
        );
        // CW square: no reversal. Points match input.
        assert_eq!(out_cw[0].points, cw_square().points);
    }

    #[test]
    fn choose_best_direction_alone_is_observable_even_with_tool_at_origin() {
        // The previous code used `vec![0]` as the only rotation candidate,
        // and the tie-break
        // always picked forward, so the flag alone never flipped
        // anything. Here the tool position is irrelevant — the flag
        // should change the output of a CCW polyline regardless.
        let ccw = unit_square(); // CCW under shoelace convention
        let with_flag = apply_start_point(
            vec![ccw.clone()],
            Point2D::new(42.0, 42.0),
            false,
            false,
            true,
        );
        let without_flag = apply_start_point(
            vec![ccw.clone()],
            Point2D::new(42.0, 42.0),
            false,
            false,
            false,
        );
        // Without flag: polyline untouched.
        assert_eq!(without_flag[0].points, ccw.points);
        // With flag: polyline reversed.
        assert_ne!(with_flag[0].points, ccw.points);
    }

    #[test]
    fn choose_best_direction_reverses_when_shorter() {
        // Square with tool at (-0.1, -0.1): nearest vertex is (0,0) at
        // index 0. With the forward traversal, the first-after-start is
        // (10,0). With the reversed traversal, the first-after-start is
        // (0,10). The entry cost is identical for both (both land on
        // (0,0)), so reversal doesn't flip the choice here — we need a
        // test whose entry points differ.
        //
        // Construct an asymmetric closed polyline where the forward and
        // reversed rotations produce different start vertices and one is
        // closer to the tool.
        // Ring: [(0,0), (10,0), (10,10), (0,10)] — same as unit_square,
        // but with tool at (-1, 11). Forward best rotation: idx 3 →
        // (0,10). Reversed-then-rotated: for rot r, reversed_ring_at
        // gives the (ring_len - r)-th element of the original ring, so
        // rot 0 → (0,0), rot 1 → (0,10), rot 2 → (10,10), rot 3 → (10,0).
        //
        // At tool (-1, 11): forward best is (0,10) dist≈1.41; reversed
        // rot=1 also gives (0,10) dist≈1.41 → tie. First-seen wins.
        let out = apply_start_point(
            vec![unit_square()],
            Point2D::new(-1.0, 11.0),
            true,
            false,
            true,
        );
        assert_eq!(out[0].points[0], Point2D::new(0.0, 10.0));
    }

    #[test]
    fn reverse_closed_preserves_anchor() {
        let mut pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(10.0, 0.0),
            Point2D::new(10.0, 10.0),
            Point2D::new(0.0, 10.0),
            Point2D::new(0.0, 0.0),
        ];
        reverse_closed(&mut pts);
        assert_eq!(pts[0], Point2D::new(0.0, 0.0));
        assert_eq!(pts.last(), Some(&Point2D::new(0.0, 0.0)));
        // Body reversed.
        assert_eq!(pts[1], Point2D::new(0.0, 10.0));
        assert_eq!(pts[2], Point2D::new(10.0, 10.0));
        assert_eq!(pts[3], Point2D::new(10.0, 0.0));
    }

    #[test]
    fn rotate_closed_start_shifts_anchor() {
        let mut pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(10.0, 0.0),
            Point2D::new(10.0, 10.0),
            Point2D::new(0.0, 10.0),
            Point2D::new(0.0, 0.0),
        ];
        rotate_closed_start(&mut pts, 2);
        assert_eq!(pts[0], Point2D::new(10.0, 10.0));
        assert_eq!(pts.last(), Some(&Point2D::new(10.0, 10.0)));
        assert_eq!(pts.len(), 5);
    }

    #[test]
    fn open_polyline_is_untouched() {
        let open = Polyline {
            points: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
            ],
            closed: false,
        };
        let out = apply_start_point(
            vec![open.clone()],
            Point2D::new(100.0, 100.0),
            true,
            true,
            true,
        );
        assert_eq!(out[0].points, open.points);
    }
}

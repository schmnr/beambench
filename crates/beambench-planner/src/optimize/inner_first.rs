//! `inner_first` — reorder polylines so inner (contained) shapes emit
//! before their outer (containing) shapes.
//!
//! Algorithm: compute the containment DAG via point-in-polygon, then
//! stable-sort by **nesting depth descending**. A polyline with depth 2
//! (two ancestors contain it) emits before one with depth 1, which
//! emits before one with depth 0. This guarantees children precede
//! parents without needing a true topological sort — depth is the only
//! constraint that matters for inner-first ordering.
//!
//! Only closed polylines participate in containment. Open polylines
//! have depth 0 and preserve their relative position among peers.
//!
//! Stable sort preserves the caller's prior ordering among items of
//! equal depth. This matters when `inner_first` composes with
//! `direction_order` or the upstream priority seed.

use crate::optimize::order::Orderable;
use beambench_common::geometry::Point2D;

/// If `enabled`, reorder so deeper-nested closed polylines emit first.
/// No-op when `enabled` is false or the input has fewer than two items.
pub fn reorder_inner_first<T: Orderable>(items: Vec<T>, enabled: bool) -> Vec<T> {
    if !enabled || items.len() < 2 {
        return items;
    }

    let n = items.len();
    // depth[j] = number of other closed polylines that fully contain j.
    //
    // Only closed polylines participate in containment as EITHER the
    // container OR the containee. Open polylines would silently gain
    // a positive depth under a concave closed parent (their vertices
    // can sit inside even when the path clearly isn't a hole), which
    // is the wrong semantic for "inner first". Keep them at depth 0
    // so the stable sort leaves their relative order untouched.
    let mut depth: Vec<usize> = vec![0; n];
    for j in 0..n {
        if !items[j].closed() || items[j].points().is_empty() {
            continue;
        }
        for (i, outer) in items.iter().enumerate() {
            if i == j || !outer.closed() {
                continue;
            }
            if polyline_contains(outer.points(), items[j].points()) {
                depth[j] += 1;
            }
        }
    }

    let mut indexed: Vec<(usize, T)> = items.into_iter().enumerate().collect();
    // Stable descending sort by depth. `sort_by` is stable.
    indexed.sort_by(|(ia, _), (ib, _)| depth[*ib].cmp(&depth[*ia]));
    indexed.into_iter().map(|(_, item)| item).collect()
}

/// Check whether `outer` (treated as a closed polygon) fully contains
/// `inner`. A vertex-only test is not sufficient for concave outer
/// polygons: an inner's edges can cross an outer's concavity even when
/// every inner vertex happens to sit inside. The check is therefore
/// two-part:
///
///  1. Every vertex of `inner` is inside `outer` (point-in-polygon).
///  2. No edge of `inner` crosses any edge of `outer` (segment-segment
///     intersection test, ignoring shared endpoints).
///
/// Both must hold. If either fails, `inner` is not fully contained.
/// Caller is responsible for passing closed geometry; the function
/// doesn't panic on open polylines but the result is undefined.
fn polyline_contains(outer: &[Point2D], inner: &[Point2D]) -> bool {
    if inner.is_empty() || outer.len() < 3 {
        return false;
    }
    if !inner.iter().all(|pt| point_in_polyline(pt.x, pt.y, outer)) {
        return false;
    }
    // No inner edge may cross an outer edge. Edges are the vertex pairs
    // [i, i+1] for i in 0..n-1 — the closing-duplicate convention means
    // the last pair is already the closing edge.
    let outer_n = outer.len() - 1;
    let inner_n = inner.len() - 1;
    for i in 0..inner_n {
        let a1 = inner[i];
        let a2 = inner[i + 1];
        for j in 0..outer_n {
            let b1 = outer[j];
            let b2 = outer[j + 1];
            if segments_intersect_strict(a1, a2, b1, b2) {
                return false;
            }
        }
    }
    true
}

/// Strict (open) segment-segment intersection: returns true only when
/// the two open segments (a1,a2) and (b1,b2) cross in their interiors
/// or at a T-junction that isn't a shared endpoint. Shared endpoints
/// alone don't count — adjacent polyline edges meet at shared vertices
/// by construction, and a genuine cross-through always produces a
/// parameter in (0,1) strictly.
fn segments_intersect_strict(a1: Point2D, a2: Point2D, b1: Point2D, b2: Point2D) -> bool {
    let rx = a2.x - a1.x;
    let ry = a2.y - a1.y;
    let sx = b2.x - b1.x;
    let sy = b2.y - b1.y;
    let denom = rx * sy - ry * sx;
    // Parallel / collinear segments: treat as non-crossing for this
    // heuristic. Coincident edges between distinct polylines are
    // handled by `remove_overlapping`, not here.
    if denom.abs() < 1e-12 {
        return false;
    }
    let dx = b1.x - a1.x;
    let dy = b1.y - a1.y;
    let t = (dx * sy - dy * sx) / denom;
    let u = (dx * ry - dy * rx) / denom;
    // Strict interior intersection: both params in (epsilon, 1-epsilon).
    // Epsilon excludes near-endpoint touches that are shared-vertex
    // artifacts rather than genuine crossings.
    const EPS: f64 = 1e-9;
    t > EPS && t < 1.0 - EPS && u > EPS && u < 1.0 - EPS
}

/// Ray-casting point-in-polygon. Matches the local helper in `builder.rs`
/// (duplicated here to keep this module self-contained).
fn point_in_polyline(px: f64, py: f64, pts: &[Point2D]) -> bool {
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (pts[i].x, pts[i].y);
        let (xj, yj) = (pts[j].x, pts[j].y);
        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::path::Polyline;

    fn closed_square(x: f64, y: f64, size: f64) -> Polyline {
        Polyline {
            points: vec![
                Point2D::new(x, y),
                Point2D::new(x + size, y),
                Point2D::new(x + size, y + size),
                Point2D::new(x, y + size),
                Point2D::new(x, y),
            ],
            closed: true,
        }
    }

    #[test]
    fn disabled_is_noop() {
        let a = closed_square(0.0, 0.0, 100.0);
        let b = closed_square(10.0, 10.0, 10.0);
        let input = vec![a.clone(), b.clone()];
        let out = reorder_inner_first(input, false);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].points[0], a.points[0]);
        assert_eq!(out[1].points[0], b.points[0]);
    }

    #[test]
    fn single_item_is_noop() {
        let a = closed_square(0.0, 0.0, 100.0);
        let out = reorder_inner_first(vec![a.clone()], true);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].points[0], a.points[0]);
    }

    #[test]
    fn nested_child_emits_before_parent() {
        // Outer square contains inner square.
        let outer = closed_square(0.0, 0.0, 100.0);
        let inner = closed_square(25.0, 25.0, 50.0);
        // Input has outer first; flag should reorder so inner emits first.
        let out = reorder_inner_first(vec![outer.clone(), inner.clone()], true);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].points[0], inner.points[0], "inner should be first");
        assert_eq!(out[1].points[0], outer.points[0], "outer should be last");
    }

    #[test]
    fn three_levels_of_nesting_emit_deepest_first() {
        let outer = closed_square(0.0, 0.0, 100.0);
        let middle = closed_square(20.0, 20.0, 60.0);
        let inner = closed_square(40.0, 40.0, 20.0);
        let out = reorder_inner_first(vec![outer.clone(), middle.clone(), inner.clone()], true);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].points[0], inner.points[0]);
        assert_eq!(out[1].points[0], middle.points[0]);
        assert_eq!(out[2].points[0], outer.points[0]);
    }

    #[test]
    fn siblings_preserve_relative_order() {
        // Two disjoint inner shapes at the same nesting depth — stable sort
        // should keep their input order.
        let a = closed_square(10.0, 10.0, 5.0);
        let b = closed_square(50.0, 50.0, 5.0);
        let out = reorder_inner_first(vec![a.clone(), b.clone()], true);
        assert_eq!(out[0].points[0], a.points[0]);
        assert_eq!(out[1].points[0], b.points[0]);
    }

    #[test]
    fn open_polyline_inside_closed_parent_is_not_reordered() {
        // Regression: the pre-fix implementation gave an open polyline
        // inside a closed parent a positive nesting depth, moving it
        // ahead of the parent. Open polylines should participate
        // neither as container nor as containee.
        let outer = closed_square(0.0, 0.0, 100.0);
        let open_inside = Polyline {
            points: vec![
                Point2D::new(10.0, 10.0),
                Point2D::new(20.0, 10.0),
                Point2D::new(20.0, 20.0),
            ],
            closed: false,
        };
        // Input: outer first, open_inside second.
        let out = reorder_inner_first(vec![outer.clone(), open_inside.clone()], true);
        // Depth-0 stable sort must preserve the input order: outer then open.
        assert_eq!(out[0].points[0], outer.points[0]);
        assert_eq!(out[1].points[0], open_inside.points[0]);
        assert!(!out[1].closed);
    }

    #[test]
    fn concave_parent_rejects_crossing_inner() {
        // U-shaped concave polygon: a rectangle with a rectangular notch
        // cut out of the top edge. An inner polyline whose vertices all
        // happen to sit inside the enclosing rectangle but whose edges
        // cross the notch boundary is NOT fully contained.
        //
        // Outer U (closing duplicate at end):
        //   (0,0) → (100,0) → (100,100) → (60,100) → (60,40) → (40,40)
        //   → (40,100) → (0,100) → (0,0)
        let outer = Polyline {
            points: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(100.0, 0.0),
                Point2D::new(100.0, 100.0),
                Point2D::new(60.0, 100.0),
                Point2D::new(60.0, 40.0),
                Point2D::new(40.0, 40.0),
                Point2D::new(40.0, 100.0),
                Point2D::new(0.0, 100.0),
                Point2D::new(0.0, 0.0),
            ],
            closed: true,
        };
        // Triangle whose vertices sit inside the outer's bounding box
        // (and technically inside the U itself) but whose edges cross
        // the notch walls at x=40 and x=60 in the y∈(40,100) gap.
        // Vertices chosen to all satisfy point-in-polygon on the U:
        //   (20,20)   — left arm
        //   (80,20)   — right arm
        //   (50,30)   — bottom bar
        // Edges (20,20)→(80,20) at y=20 is fully inside (below notch);
        // (80,20)→(50,30), (50,30)→(20,20) also below. All three
        // vertices and edges live fully inside — this triangle IS
        // contained. We need a crossing case, so move one vertex
        // outside the notch floor into the gap.
        let crossing = Polyline {
            points: vec![
                Point2D::new(20.0, 20.0), // inside left arm
                Point2D::new(80.0, 20.0), // inside right arm
                Point2D::new(50.0, 80.0), // inside the NOTCH GAP, outside U
                Point2D::new(20.0, 20.0),
            ],
            closed: true,
        };
        // First verify the "crossing" polyline is actually not contained:
        // (50,80) sits in the cut-out, which is outside the U. Without
        // the edge-intersection check, the two other vertices would
        // still give depth-via-PIP=1 for the ones inside.
        let depth = {
            let items = vec![outer.clone(), crossing.clone()];
            let out = reorder_inner_first(items, true);
            // If `crossing` were truly depth-1, it would emit first.
            // Since it's not contained (edges cross the notch), it
            // should NOT be depth-1 and the outer (depth 0) comes first.
            out[0].points[0] == outer.points[0]
        };
        assert!(
            depth,
            "outer should emit first because crossing polyline is not truly contained"
        );
    }

    #[test]
    fn concave_parent_accepts_truly_contained_inner() {
        // Paired with `concave_parent_rejects_crossing_inner`: a small
        // square entirely inside the U's left arm should still be
        // depth-1 and emit before the outer.
        let outer = Polyline {
            points: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(100.0, 0.0),
                Point2D::new(100.0, 100.0),
                Point2D::new(60.0, 100.0),
                Point2D::new(60.0, 40.0),
                Point2D::new(40.0, 40.0),
                Point2D::new(40.0, 100.0),
                Point2D::new(0.0, 100.0),
                Point2D::new(0.0, 0.0),
            ],
            closed: true,
        };
        let inner = closed_square(5.0, 5.0, 10.0); // entirely in left arm
        let out = reorder_inner_first(vec![outer.clone(), inner.clone()], true);
        assert_eq!(out[0].points[0], inner.points[0]);
        assert_eq!(out[1].points[0], outer.points[0]);
    }

    #[test]
    fn open_polyline_does_not_contain() {
        // An open polyline with points at the corners of a region should
        // not be treated as containing anything.
        let open = Polyline {
            points: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(100.0, 0.0),
                Point2D::new(100.0, 100.0),
                Point2D::new(0.0, 100.0),
            ],
            closed: false,
        };
        let inner = closed_square(25.0, 25.0, 50.0);
        let out = reorder_inner_first(vec![open.clone(), inner.clone()], true);
        // Neither is contained in anything (open is open; inner not contained
        // in the open one). Stable order preserves input.
        assert_eq!(out[0].points[0], open.points[0]);
        assert_eq!(out[1].points[0], inner.points[0]);
    }

    #[test]
    fn disjoint_shapes_preserve_input_order() {
        let a = closed_square(0.0, 0.0, 5.0);
        let b = closed_square(100.0, 100.0, 5.0);
        let out = reorder_inner_first(vec![a.clone(), b.clone()], true);
        assert_eq!(out[0].points[0], a.points[0]);
        assert_eq!(out[1].points[0], b.points[0]);
    }
}

//! `remove_overlapping` — drop near-duplicate polylines before ordering.
//!
//! Overlap removal catches shapes that
//! would burn the same geometry twice (most commonly: two imported
//! copies of the same contour, or adjacent shapes that share a single
//! traced edge after a union).
//!
//! Phase 4b scope: polyline-level near-duplicate detection only.
//! Two polylines match when they share `closed`, point count, and
//! every corresponding vertex is within `tolerance_mm` — checked in
//! both forward and reverse orientation so opposite-winding duplicates
//! still cancel. Segment-level overlap (shared edges between
//! differently-shaped polylines) is out of scope for 4b.
//!
//! Stable keep-first: when duplicates are found, the earliest item in
//! the input survives. Callers that want a different survivor order
//! (e.g. prefer closed over open) must pre-sort before calling.

use beambench_common::geometry::Point2D;

use crate::optimize::order::Orderable;

/// If `enabled`, drop polylines that near-duplicate an earlier polyline
/// within `tolerance_mm`. No-op when disabled, when `tolerance_mm <= 0`,
/// or when the input has fewer than two items.
pub fn remove_near_duplicates<T: Orderable>(
    items: Vec<T>,
    enabled: bool,
    tolerance_mm: f64,
) -> Vec<T> {
    if !enabled || tolerance_mm <= 0.0 || items.len() < 2 {
        return items;
    }
    let tol_sq = tolerance_mm * tolerance_mm;
    let n = items.len();
    let mut keep: Vec<bool> = vec![true; n];

    for i in 0..n {
        if !keep[i] {
            continue;
        }
        for j in (i + 1)..n {
            if !keep[j] {
                continue;
            }
            if near_duplicate(
                items[i].points(),
                items[i].closed(),
                items[j].points(),
                items[j].closed(),
                tol_sq,
            ) {
                keep[j] = false;
            }
        }
    }

    items
        .into_iter()
        .zip(keep)
        .filter_map(|(item, k)| if k { Some(item) } else { None })
        .collect()
}

/// Return true when `a` and `b` describe the same polyline within
/// `tol_sq` (squared tolerance). Requires matching `closed` flags and
/// matching point counts.
///
/// - **Open polylines**: accept forward vertex correspondence or full
///   reversal only. An open polyline's endpoints carry semantic meaning
///   (start and finish), so a rotation would describe a different
///   traversal even if the vertex set matched.
/// - **Closed polylines**: accept *any* rotational alignment in either
///   direction. The closing-duplicate convention (first vertex equals
///   last) means the same burned ring can be imported twice with
///   different start vertices, e.g. `[A,B,C,D,A]` and `[B,C,D,A,B]` —
///   both trace the same path but vertex-for-vertex forward comparison
///   would miss the duplicate. We strip the closing duplicate, then try
///   every rotational offset (forward and reversed) against `a`'s ring.
fn near_duplicate(
    a: &[Point2D],
    a_closed: bool,
    b: &[Point2D],
    b_closed: bool,
    tol_sq: f64,
) -> bool {
    if a_closed != b_closed {
        return false;
    }
    if a.len() != b.len() || a.len() < 2 {
        return false;
    }
    if !a_closed {
        let forward = a.iter().zip(b.iter()).all(|(p, q)| dist_sq(p, q) <= tol_sq);
        if forward {
            return true;
        }
        return a
            .iter()
            .zip(b.iter().rev())
            .all(|(p, q)| dist_sq(p, q) <= tol_sq);
    }

    // Closed: strip the closing-duplicate anchor on both rings and
    // compare every rotation × direction pair.
    let ring_a = &a[..a.len() - 1];
    let ring_b = &b[..b.len() - 1];
    let n = ring_a.len();
    if n == 0 {
        return true;
    }
    for offset in 0..n {
        if rotated_forward_match(ring_a, ring_b, offset, tol_sq)
            || rotated_reverse_match(ring_a, ring_b, offset, tol_sq)
        {
            return true;
        }
    }
    false
}

/// Check whether `ring_a[i]` matches `ring_b[(i + offset) % n]` for all `i`.
fn rotated_forward_match(
    ring_a: &[Point2D],
    ring_b: &[Point2D],
    offset: usize,
    tol_sq: f64,
) -> bool {
    let n = ring_a.len();
    (0..n).all(|i| dist_sq(&ring_a[i], &ring_b[(i + offset) % n]) <= tol_sq)
}

/// Check whether `ring_a[i]` matches `ring_b` traversed in reverse
/// starting at `offset`: `ring_b[(offset + n - i) % n]` for all `i`.
fn rotated_reverse_match(
    ring_a: &[Point2D],
    ring_b: &[Point2D],
    offset: usize,
    tol_sq: f64,
) -> bool {
    let n = ring_a.len();
    (0..n).all(|i| dist_sq(&ring_a[i], &ring_b[(offset + n - i) % n]) <= tol_sq)
}

fn dist_sq(a: &Point2D, b: &Point2D) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::path::Polyline;

    fn square(x: f64, y: f64, size: f64) -> Polyline {
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
        let a = square(0.0, 0.0, 10.0);
        let b = square(0.0, 0.0, 10.0);
        let out = remove_near_duplicates(vec![a, b], false, 0.05);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn zero_tolerance_is_noop() {
        let a = square(0.0, 0.0, 10.0);
        let b = square(0.0, 0.0, 10.0);
        let out = remove_near_duplicates(vec![a, b], true, 0.0);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn single_item_is_noop() {
        let a = square(0.0, 0.0, 10.0);
        let out = remove_near_duplicates(vec![a], true, 0.05);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn exact_duplicate_is_removed() {
        let a = square(0.0, 0.0, 10.0);
        let b = square(0.0, 0.0, 10.0);
        let out = remove_near_duplicates(vec![a.clone(), b], true, 0.05);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].points[0], a.points[0]);
    }

    #[test]
    fn within_tolerance_removed() {
        let a = square(0.0, 0.0, 10.0);
        let mut b = square(0.0, 0.0, 10.0);
        // Nudge every point by 0.01 (well under 0.05 tolerance).
        for p in &mut b.points {
            p.x += 0.01;
            p.y += 0.01;
        }
        let out = remove_near_duplicates(vec![a.clone(), b], true, 0.05);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn outside_tolerance_kept() {
        let a = square(0.0, 0.0, 10.0);
        let b = square(1.0, 1.0, 10.0);
        let out = remove_near_duplicates(vec![a, b], true, 0.05);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn different_point_count_kept() {
        let a = square(0.0, 0.0, 10.0);
        let b = Polyline {
            points: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
                Point2D::new(0.0, 0.0),
            ],
            closed: true,
        };
        let out = remove_near_duplicates(vec![a, b], true, 0.05);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn mixed_closed_and_open_kept() {
        // Same vertices, one closed one open → not a duplicate.
        let a = Polyline {
            points: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
            ],
            closed: true,
        };
        let b = Polyline {
            points: a.points.clone(),
            closed: false,
        };
        let out = remove_near_duplicates(vec![a, b], true, 0.05);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn reverse_orientation_duplicate_removed() {
        // Same closed shape with reversed vertex order.
        let a = square(0.0, 0.0, 10.0);
        let mut b_pts = a.points.clone();
        b_pts.reverse();
        let b = Polyline {
            points: b_pts,
            closed: true,
        };
        let out = remove_near_duplicates(vec![a, b], true, 0.05);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn rotated_closed_duplicate_is_removed() {
        // Two closed squares describing the same ring but with
        // different start vertices. Pre-fix: these would be kept as
        // distinct because forward vertex comparison fails.
        //
        // Canonical square ring: [(0,0), (10,0), (10,10), (0,10)] with
        // a closing duplicate at end.
        let a = Polyline {
            points: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
                Point2D::new(0.0, 10.0),
                Point2D::new(0.0, 0.0),
            ],
            closed: true,
        };
        // Same ring rotated to start at (10, 0).
        let b = Polyline {
            points: vec![
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
                Point2D::new(0.0, 10.0),
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
            ],
            closed: true,
        };
        let out = remove_near_duplicates(vec![a.clone(), b], true, 0.05);
        assert_eq!(out.len(), 1);
        // First survivor is `a` (keep-first rule).
        assert_eq!(out[0].points[0], Point2D::new(0.0, 0.0));
    }

    #[test]
    fn rotated_and_reversed_closed_duplicate_is_removed() {
        // Same ring, rotated AND reversed (opposite winding).
        let a = Polyline {
            points: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
                Point2D::new(0.0, 10.0),
                Point2D::new(0.0, 0.0),
            ],
            closed: true,
        };
        // Start at (10,10), traverse counterclockwise.
        let b = Polyline {
            points: vec![
                Point2D::new(10.0, 10.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(0.0, 0.0),
                Point2D::new(0.0, 10.0),
                Point2D::new(10.0, 10.0),
            ],
            closed: true,
        };
        let out = remove_near_duplicates(vec![a, b], true, 0.05);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn open_polyline_rotation_is_not_considered_duplicate() {
        // Open polylines carry ordered endpoint semantics; a rotated
        // version with different start/end is a different path. The
        // closed-ring rotation logic must NOT apply here.
        let a = Polyline {
            points: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
            ],
            closed: false,
        };
        let b = Polyline {
            points: vec![
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
                Point2D::new(0.0, 0.0),
            ],
            closed: false,
        };
        let out = remove_near_duplicates(vec![a, b], true, 0.05);
        assert_eq!(out.len(), 2, "rotated open polyline is a different path");
    }

    #[test]
    fn three_items_two_dupes_one_unique() {
        let a = square(0.0, 0.0, 10.0);
        let b = square(0.0, 0.0, 10.0); // dup of a
        let c = square(50.0, 50.0, 10.0); // unique
        let out = remove_near_duplicates(vec![a, b, c.clone()], true, 0.05);
        assert_eq!(out.len(), 2);
        // First survivor is `a`, second is `c`.
        assert_eq!(out[1].points[0], c.points[0]);
    }
}

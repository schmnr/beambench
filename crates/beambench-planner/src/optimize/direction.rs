//! `direction_order` — stable axial sort of polylines by bounding box.
//!
//! Implements directional cut ordering.
//! Each variant picks a coordinate extremum; the sort is stable so any
//! upstream ordering (priority, inner-first) acts as the tiebreaker
//! for items that share the chosen key.

use beambench_core::DirectionOrder;
use std::cmp::Ordering;

use crate::optimize::order::Orderable;

/// Sort `items` in place (returning a new `Vec`) by the axis key selected
/// by `dir`. `DirectionOrder::None` is a no-op.
pub fn apply_direction_order<T: Orderable>(mut items: Vec<T>, dir: DirectionOrder) -> Vec<T> {
    if matches!(dir, DirectionOrder::None) || items.len() < 2 {
        return items;
    }
    items.sort_by(|a, b| {
        let ka = key(a, dir);
        let kb = key(b, dir);
        ka.partial_cmp(&kb).unwrap_or(Ordering::Equal)
    });
    items
}

/// Compute the scalar sort key for `item` under `dir`.
///
/// - `TopDown`  : ascending min-Y (smaller-Y first).
/// - `BottomUp` : descending max-Y (expressed as negative max-Y so a single
///   ascending sort works for every variant).
/// - `LeftRight`: ascending min-X.
/// - `RightLeft`: descending max-X (negated).
fn key<T: Orderable>(item: &T, dir: DirectionOrder) -> f64 {
    let pts = item.points();
    if pts.is_empty() {
        return 0.0;
    }
    match dir {
        DirectionOrder::None => 0.0,
        DirectionOrder::TopDown => pts.iter().map(|p| p.y).fold(f64::INFINITY, f64::min),
        DirectionOrder::BottomUp => -pts.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max),
        DirectionOrder::LeftRight => pts.iter().map(|p| p.x).fold(f64::INFINITY, f64::min),
        DirectionOrder::RightLeft => -pts.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::geometry::Point2D;
    use beambench_common::path::Polyline;

    fn line(x1: f64, y1: f64, x2: f64, y2: f64) -> Polyline {
        Polyline {
            points: vec![Point2D::new(x1, y1), Point2D::new(x2, y2)],
            closed: false,
        }
    }

    fn first_point(p: &Polyline) -> Point2D {
        p.points[0]
    }

    #[test]
    fn none_is_noop() {
        let a = line(0.0, 100.0, 10.0, 100.0);
        let b = line(0.0, 0.0, 10.0, 0.0);
        let out = apply_direction_order(vec![a.clone(), b.clone()], DirectionOrder::None);
        assert_eq!(out[0].points[0], a.points[0]);
        assert_eq!(out[1].points[0], b.points[0]);
    }

    #[test]
    fn top_down_sorts_ascending_min_y() {
        let bottom = line(0.0, 100.0, 10.0, 100.0);
        let top = line(0.0, 0.0, 10.0, 0.0);
        let middle = line(0.0, 50.0, 10.0, 50.0);
        // Input in reverse vertical order.
        let out = apply_direction_order(
            vec![bottom.clone(), middle.clone(), top.clone()],
            DirectionOrder::TopDown,
        );
        assert_eq!(first_point(&out[0]).y, 0.0);
        assert_eq!(first_point(&out[1]).y, 50.0);
        assert_eq!(first_point(&out[2]).y, 100.0);
    }

    #[test]
    fn bottom_up_sorts_descending_max_y() {
        let bottom = line(0.0, 100.0, 10.0, 100.0);
        let top = line(0.0, 0.0, 10.0, 0.0);
        let out =
            apply_direction_order(vec![top.clone(), bottom.clone()], DirectionOrder::BottomUp);
        // Bottom (y=100) first, top (y=0) second.
        assert_eq!(first_point(&out[0]).y, 100.0);
        assert_eq!(first_point(&out[1]).y, 0.0);
    }

    #[test]
    fn left_right_sorts_ascending_min_x() {
        let right = line(100.0, 0.0, 110.0, 0.0);
        let left = line(0.0, 0.0, 10.0, 0.0);
        let out =
            apply_direction_order(vec![right.clone(), left.clone()], DirectionOrder::LeftRight);
        assert_eq!(first_point(&out[0]).x, 0.0);
        assert_eq!(first_point(&out[1]).x, 100.0);
    }

    #[test]
    fn right_left_sorts_descending_max_x() {
        let right = line(100.0, 0.0, 110.0, 0.0);
        let left = line(0.0, 0.0, 10.0, 0.0);
        let out =
            apply_direction_order(vec![left.clone(), right.clone()], DirectionOrder::RightLeft);
        assert_eq!(first_point(&out[0]).x, 100.0);
        assert_eq!(first_point(&out[1]).x, 0.0);
    }

    #[test]
    fn stable_for_tied_keys() {
        // Two polylines at the same Y — order should follow input.
        let a = line(0.0, 50.0, 10.0, 50.0);
        let b = line(100.0, 50.0, 110.0, 50.0);
        let out = apply_direction_order(vec![a.clone(), b.clone()], DirectionOrder::TopDown);
        assert_eq!(first_point(&out[0]).x, 0.0);
        assert_eq!(first_point(&out[1]).x, 100.0);
    }

    #[test]
    fn empty_polyline_keeps_relative_order() {
        let empty = Polyline {
            points: vec![],
            closed: false,
        };
        let real = line(0.0, 10.0, 10.0, 10.0);
        let out = apply_direction_order(vec![empty.clone(), real.clone()], DirectionOrder::TopDown);
        // Empty gets key 0.0, real gets key 10.0 → empty first.
        assert_eq!(out[0].points.len(), 0);
        assert_eq!(out[1].points[0].y, 10.0);
    }
}

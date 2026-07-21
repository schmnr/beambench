//! Nearest-neighbor polyline ordering.
//!
//! Post-M1 the per-layer ordering pass has exactly two behaviors —
//! run nearest-neighbor or leave the input untouched — driven by the
//! caller via the Phase 4c `effective_cut_strategy` gate. The old
//! `CutOrderStrategy` enum with `AsDrawn` / `Optimized` / `InsideFirst`
//! / `OutsideFirst` variants is gone; only the nearest-neighbor path
//! survived (InsideFirst and OutsideFirst had no UI, service, or IPC
//! caller) and the AsDrawn branch collapses to an identity at the call
//! site.

use beambench_common::geometry::Point2D;
use beambench_common::path::Polyline;

/// Anything that can be ordered as a vector path. Implemented for
/// [`Polyline`] and for the planner's `TaggedPolyline` wrapper.
pub trait Orderable {
    fn points(&self) -> &[Point2D];
    fn closed(&self) -> bool;
    fn points_mut(&mut self) -> &mut Vec<Point2D>;
}

impl Orderable for Polyline {
    fn points(&self) -> &[Point2D] {
        &self.points
    }
    fn closed(&self) -> bool {
        self.closed
    }
    fn points_mut(&mut self) -> &mut Vec<Point2D> {
        &mut self.points
    }
}

/// Generic nearest-next ordering: starting from `(0, 0)`, at each step
/// pick the item whose start (or, for open polylines, whichever end) is
/// closest to the current tool position, reversing open polylines when
/// entering from the far end is shorter. Closed polylines keep their
/// vertex order; `start_point::apply_start_point` is the pass that
/// rotates their entry vertex, and it runs after this one so it sees
/// the final nearest-neighbor sequence.
pub fn order_nearest_next_generic<T: Orderable>(mut items: Vec<T>) -> Vec<T> {
    if items.is_empty() {
        return vec![];
    }

    let mut result = Vec::with_capacity(items.len());
    let mut current_pos = Point2D::new(0.0, 0.0);

    while !items.is_empty() {
        let mut nearest_idx = 0;
        let mut nearest_distance = f64::MAX;
        let mut should_reverse = false;

        for (idx, item) in items.iter().enumerate() {
            if item.points().is_empty() {
                continue;
            }

            let first_point = &item.points()[0];
            let distance_to_first = current_pos.distance_to(first_point);

            if distance_to_first < nearest_distance {
                nearest_distance = distance_to_first;
                nearest_idx = idx;
                should_reverse = false;
            }

            if !item.closed() && item.points().len() > 1 {
                let last_point = &item.points()[item.points().len() - 1];
                let distance_to_last = current_pos.distance_to(last_point);

                if distance_to_last < nearest_distance {
                    nearest_distance = distance_to_last;
                    nearest_idx = idx;
                    should_reverse = true;
                }
            }
        }

        let mut nearest = items.remove(nearest_idx);

        if should_reverse {
            nearest.points_mut().reverse();
        }

        if let Some(last_point) = nearest.points().last() {
            current_pos = *last_point;
        }

        result.push(nearest);
    }

    result
}

/// Thin wrapper so callers that already hold `Vec<Polyline>` don't need
/// the turbofish.
pub fn order_nearest_next(polylines: Vec<Polyline>) -> Vec<Polyline> {
    order_nearest_next_generic(polylines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_empty() {
        let result = order_nearest_next(vec![]);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn single_polyline_returns_same() {
        let polyline = Polyline {
            points: vec![Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)],
            closed: false,
        };

        let result = order_nearest_next(vec![polyline.clone()]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].points, polyline.points);
    }

    #[test]
    fn two_polylines_nearest_first() {
        let near = Polyline {
            points: vec![Point2D::new(5.0, 5.0), Point2D::new(10.0, 10.0)],
            closed: false,
        };
        let far = Polyline {
            points: vec![Point2D::new(100.0, 100.0), Point2D::new(110.0, 110.0)],
            closed: false,
        };

        let result = order_nearest_next(vec![far.clone(), near.clone()]);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].points[0], Point2D::new(5.0, 5.0));
        assert_eq!(result[1].points[0], Point2D::new(100.0, 100.0));
    }

    #[test]
    fn three_polylines_nearest_next_ordering() {
        let first = Polyline {
            points: vec![Point2D::new(1.0, 1.0), Point2D::new(2.0, 2.0)],
            closed: false,
        };
        let second = Polyline {
            points: vec![Point2D::new(3.0, 3.0), Point2D::new(4.0, 4.0)],
            closed: false,
        };
        let third = Polyline {
            points: vec![Point2D::new(5.0, 5.0), Point2D::new(6.0, 6.0)],
            closed: false,
        };

        let result = order_nearest_next(vec![third.clone(), second.clone(), first.clone()]);

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].points[0], Point2D::new(1.0, 1.0));
        assert_eq!(result[1].points[0], Point2D::new(3.0, 3.0));
        assert_eq!(result[2].points[0], Point2D::new(5.0, 5.0));
    }

    #[test]
    fn open_polyline_reverses_if_closer() {
        // Polyline whose end is closer to origin than its start
        let polyline = Polyline {
            points: vec![
                Point2D::new(100.0, 100.0),
                Point2D::new(50.0, 50.0),
                Point2D::new(1.0, 1.0),
            ],
            closed: false,
        };

        let result = order_nearest_next(vec![polyline]);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].points[0], Point2D::new(1.0, 1.0));
        assert_eq!(result[0].points[2], Point2D::new(100.0, 100.0));
    }

    #[test]
    fn closed_polyline_not_reversed() {
        // Closed polylines don't reverse — their entry vertex is the
        // responsibility of `start_point::apply_start_point`, which
        // runs after this pass.
        let polyline = Polyline {
            points: vec![
                Point2D::new(100.0, 100.0),
                Point2D::new(50.0, 50.0),
                Point2D::new(1.0, 1.0),
            ],
            closed: true,
        };

        let result = order_nearest_next(vec![polyline.clone()]);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].points[0], Point2D::new(100.0, 100.0));
    }

    #[test]
    fn empty_polyline_handled() {
        let empty = Polyline {
            points: vec![],
            closed: false,
        };
        let valid = Polyline {
            points: vec![Point2D::new(10.0, 10.0)],
            closed: false,
        };

        let result = order_nearest_next(vec![empty.clone(), valid.clone()]);

        assert_eq!(result.len(), 2);
    }

    #[test]
    fn ordering_updates_current_position() {
        let first = Polyline {
            points: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
            closed: false,
        };
        let second = Polyline {
            points: vec![Point2D::new(12.0, 0.0), Point2D::new(20.0, 0.0)],
            closed: false,
        };
        let third = Polyline {
            points: vec![Point2D::new(22.0, 0.0), Point2D::new(30.0, 0.0)],
            closed: false,
        };

        let result = order_nearest_next(vec![third.clone(), first.clone(), second.clone()]);

        assert_eq!(result[0].points[0].x, 0.0);
        assert_eq!(result[1].points[0].x, 12.0);
        assert_eq!(result[2].points[0].x, 22.0);
    }

    #[test]
    fn single_point_polyline() {
        let polyline = Polyline {
            points: vec![Point2D::new(5.0, 5.0)],
            closed: false,
        };

        let result = order_nearest_next(vec![polyline.clone()]);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].points[0], Point2D::new(5.0, 5.0));
    }

    #[test]
    fn reverse_detection_with_multiple_candidates() {
        // First polyline ends at (10, 0); second's end is near it.
        let first = Polyline {
            points: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
            closed: false,
        };
        let second = Polyline {
            points: vec![Point2D::new(100.0, 0.0), Point2D::new(11.0, 0.0)],
            closed: false,
        };

        let result = order_nearest_next(vec![first, second]);

        assert_eq!(result[1].points[0].x, 11.0);
        assert_eq!(result[1].points[1].x, 100.0);
    }
}

//! Alignment and distribution geometry functions.
//!
//! Each function takes a slice of [`Bounds`] and returns a [`Vec<Point2D>`] of offsets
//! to apply to each object in order to achieve the desired alignment or distribution.

use beambench_common::{Bounds, Point2D};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorAlignment {
    Left,
    Right,
    Top,
    Bottom,
    CentersXy,
    CentersV,
    CentersH,
}

fn center_x(bounds: Bounds) -> f64 {
    (bounds.min.x + bounds.max.x) / 2.0
}

fn center_y(bounds: Bounds) -> f64 {
    (bounds.min.y + bounds.max.y) / 2.0
}

pub fn align_to_anchor(
    bounds: &[Bounds],
    mode: AnchorAlignment,
    anchor_index: usize,
    movable: &[bool],
) -> Vec<Point2D> {
    if bounds.is_empty() || anchor_index >= bounds.len() || movable.len() != bounds.len() {
        return vec![Point2D::zero(); bounds.len()];
    }

    let anchor = bounds[anchor_index];
    bounds
        .iter()
        .zip(movable.iter())
        .map(|(b, can_move)| {
            if !can_move {
                return Point2D::zero();
            }
            match mode {
                AnchorAlignment::Left => Point2D::new(anchor.min.x - b.min.x, 0.0),
                AnchorAlignment::Right => Point2D::new(anchor.max.x - b.max.x, 0.0),
                AnchorAlignment::Top => Point2D::new(0.0, anchor.min.y - b.min.y),
                AnchorAlignment::Bottom => Point2D::new(0.0, anchor.max.y - b.max.y),
                AnchorAlignment::CentersXy => Point2D::new(
                    center_x(anchor) - center_x(*b),
                    center_y(anchor) - center_y(*b),
                ),
                // Vertical centers share a vertical line (same X).
                AnchorAlignment::CentersV => Point2D::new(center_x(anchor) - center_x(*b), 0.0),
                // Horizontal centers share a horizontal line (same Y).
                AnchorAlignment::CentersH => Point2D::new(0.0, center_y(anchor) - center_y(*b)),
            }
        })
        .collect()
}

/// Align objects so their left edges match the leftmost object's left edge.
pub fn align_left(bounds: &[Bounds]) -> Vec<Point2D> {
    if bounds.is_empty() {
        return vec![];
    }
    let target = bounds.iter().map(|b| b.min.x).fold(f64::INFINITY, f64::min);
    bounds
        .iter()
        .map(|b| Point2D::new(target - b.min.x, 0.0))
        .collect()
}

/// Align objects so their right edges match the rightmost object's right edge.
pub fn align_right(bounds: &[Bounds]) -> Vec<Point2D> {
    if bounds.is_empty() {
        return vec![];
    }
    let target = bounds
        .iter()
        .map(|b| b.max.x)
        .fold(f64::NEG_INFINITY, f64::max);
    bounds
        .iter()
        .map(|b| Point2D::new(target - b.max.x, 0.0))
        .collect()
}

/// Align objects so their top edges match the topmost object's top edge.
pub fn align_top(bounds: &[Bounds]) -> Vec<Point2D> {
    if bounds.is_empty() {
        return vec![];
    }
    let target = bounds.iter().map(|b| b.min.y).fold(f64::INFINITY, f64::min);
    bounds
        .iter()
        .map(|b| Point2D::new(0.0, target - b.min.y))
        .collect()
}

/// Align objects so their bottom edges match the bottommost object's bottom edge.
pub fn align_bottom(bounds: &[Bounds]) -> Vec<Point2D> {
    if bounds.is_empty() {
        return vec![];
    }
    let target = bounds
        .iter()
        .map(|b| b.max.y)
        .fold(f64::NEG_INFINITY, f64::max);
    bounds
        .iter()
        .map(|b| Point2D::new(0.0, target - b.max.y))
        .collect()
}

/// Align objects so their horizontal centers match the group's average horizontal center.
pub fn align_center_h(bounds: &[Bounds]) -> Vec<Point2D> {
    if bounds.is_empty() {
        return vec![];
    }
    let centers: Vec<f64> = bounds.iter().map(|b| (b.min.x + b.max.x) / 2.0).collect();
    let group_center = centers.iter().sum::<f64>() / centers.len() as f64;
    centers
        .iter()
        .map(|cx| Point2D::new(group_center - cx, 0.0))
        .collect()
}

/// Align objects so their vertical centers match the group's average vertical center.
pub fn align_center_v(bounds: &[Bounds]) -> Vec<Point2D> {
    if bounds.is_empty() {
        return vec![];
    }
    let centers: Vec<f64> = bounds.iter().map(|b| (b.min.y + b.max.y) / 2.0).collect();
    let group_center = centers.iter().sum::<f64>() / centers.len() as f64;
    centers
        .iter()
        .map(|cy| Point2D::new(0.0, group_center - cy))
        .collect()
}

/// Distribute objects evenly along the horizontal axis.
///
/// Objects are sorted by their horizontal center. The leftmost and rightmost
/// centers stay in place, and intermediate objects are spaced evenly between them.
pub fn distribute_h_centered(bounds: &[Bounds]) -> Vec<Point2D> {
    if bounds.len() < 3 {
        return vec![Point2D::zero(); bounds.len()];
    }

    let centers: Vec<f64> = bounds.iter().map(|b| (b.min.x + b.max.x) / 2.0).collect();

    if centers.iter().any(|c| !c.is_finite()) {
        return vec![Point2D::zero(); bounds.len()];
    }

    // Build sorted indices by center_x
    let mut indices: Vec<usize> = (0..bounds.len()).collect();
    indices.sort_by(|&a, &b| centers[a].partial_cmp(&centers[b]).unwrap());

    let first_center = centers[indices[0]];
    let last_center = centers[*indices.last().unwrap()];
    let step = (last_center - first_center) / (indices.len() - 1) as f64;

    let mut offsets = vec![Point2D::zero(); bounds.len()];
    for (rank, &original_idx) in indices.iter().enumerate() {
        let target_center = first_center + step * rank as f64;
        offsets[original_idx] = Point2D::new(target_center - centers[original_idx], 0.0);
    }
    offsets
}

/// Distribute objects evenly along the vertical axis.
///
/// Objects are sorted by their vertical center. The topmost and bottommost
/// centers stay in place, and intermediate objects are spaced evenly between them.
pub fn distribute_v_centered(bounds: &[Bounds]) -> Vec<Point2D> {
    if bounds.len() < 3 {
        return vec![Point2D::zero(); bounds.len()];
    }

    let centers: Vec<f64> = bounds.iter().map(|b| (b.min.y + b.max.y) / 2.0).collect();

    if centers.iter().any(|c| !c.is_finite()) {
        return vec![Point2D::zero(); bounds.len()];
    }

    // Build sorted indices by center_y
    let mut indices: Vec<usize> = (0..bounds.len()).collect();
    indices.sort_by(|&a, &b| centers[a].partial_cmp(&centers[b]).unwrap());

    let first_center = centers[indices[0]];
    let last_center = centers[*indices.last().unwrap()];
    let step = (last_center - first_center) / (indices.len() - 1) as f64;

    let mut offsets = vec![Point2D::zero(); bounds.len()];
    for (rank, &original_idx) in indices.iter().enumerate() {
        let target_center = first_center + step * rank as f64;
        offsets[original_idx] = Point2D::new(0.0, target_center - centers[original_idx]);
    }
    offsets
}

/// Distribute objects evenly by horizontal edge spacing.
pub fn distribute_h_spaced(bounds: &[Bounds]) -> Vec<Point2D> {
    if bounds.len() < 3 {
        return vec![Point2D::zero(); bounds.len()];
    }
    if bounds
        .iter()
        .any(|b| !b.min.x.is_finite() || !b.max.x.is_finite())
    {
        return vec![Point2D::zero(); bounds.len()];
    }

    let mut indices: Vec<usize> = (0..bounds.len()).collect();
    indices.sort_by(|&a, &b| bounds[a].min.x.partial_cmp(&bounds[b].min.x).unwrap());

    let first_min = bounds[indices[0]].min.x;
    let last_max = bounds[*indices.last().unwrap()].max.x;
    let total_width: f64 = indices
        .iter()
        .map(|&idx| bounds[idx].max.x - bounds[idx].min.x)
        .sum();
    let gap = (last_max - first_min - total_width) / (indices.len() - 1) as f64;

    let mut offsets = vec![Point2D::zero(); bounds.len()];
    let mut cursor = first_min;
    for &idx in &indices {
        offsets[idx] = Point2D::new(cursor - bounds[idx].min.x, 0.0);
        cursor += bounds[idx].max.x - bounds[idx].min.x + gap;
    }
    offsets
}

/// Distribute objects evenly by vertical edge spacing.
pub fn distribute_v_spaced(bounds: &[Bounds]) -> Vec<Point2D> {
    if bounds.len() < 3 {
        return vec![Point2D::zero(); bounds.len()];
    }
    if bounds
        .iter()
        .any(|b| !b.min.y.is_finite() || !b.max.y.is_finite())
    {
        return vec![Point2D::zero(); bounds.len()];
    }

    let mut indices: Vec<usize> = (0..bounds.len()).collect();
    indices.sort_by(|&a, &b| bounds[a].min.y.partial_cmp(&bounds[b].min.y).unwrap());

    let first_min = bounds[indices[0]].min.y;
    let last_max = bounds[*indices.last().unwrap()].max.y;
    let total_height: f64 = indices
        .iter()
        .map(|&idx| bounds[idx].max.y - bounds[idx].min.y)
        .sum();
    let gap = (last_max - first_min - total_height) / (indices.len() - 1) as f64;

    let mut offsets = vec![Point2D::zero(); bounds.len()];
    let mut cursor = first_min;
    for &idx in &indices {
        offsets[idx] = Point2D::new(0.0, cursor - bounds[idx].min.y);
        cursor += bounds[idx].max.y - bounds[idx].min.y + gap;
    }
    offsets
}

/// Pack objects so their horizontal edges abut, keeping the anchor object fixed.
pub fn move_together_h(bounds: &[Bounds], anchor_index: usize) -> Vec<Point2D> {
    if bounds.is_empty() || anchor_index >= bounds.len() {
        return vec![Point2D::zero(); bounds.len()];
    }

    let mut offsets = vec![Point2D::zero(); bounds.len()];
    let anchor = bounds[anchor_index];
    let anchor_center = (anchor.min.x + anchor.max.x) / 2.0;

    let mut left: Vec<usize> = (0..bounds.len())
        .filter(|&idx| {
            idx != anchor_index && (bounds[idx].min.x + bounds[idx].max.x) / 2.0 < anchor_center
        })
        .collect();
    left.sort_by(|&a, &b| {
        let ac = (bounds[a].min.x + bounds[a].max.x) / 2.0;
        let bc = (bounds[b].min.x + bounds[b].max.x) / 2.0;
        bc.partial_cmp(&ac).unwrap()
    });

    let mut cursor = anchor.min.x;
    for idx in left {
        let width = bounds[idx].max.x - bounds[idx].min.x;
        let target_min = cursor - width;
        offsets[idx] = Point2D::new(target_min - bounds[idx].min.x, 0.0);
        cursor = target_min;
    }

    let mut right: Vec<usize> = (0..bounds.len())
        .filter(|&idx| {
            idx != anchor_index && (bounds[idx].min.x + bounds[idx].max.x) / 2.0 >= anchor_center
        })
        .collect();
    right.sort_by(|&a, &b| {
        let ac = (bounds[a].min.x + bounds[a].max.x) / 2.0;
        let bc = (bounds[b].min.x + bounds[b].max.x) / 2.0;
        ac.partial_cmp(&bc).unwrap()
    });

    cursor = anchor.max.x;
    for idx in right {
        let width = bounds[idx].max.x - bounds[idx].min.x;
        let target_min = cursor;
        offsets[idx] = Point2D::new(target_min - bounds[idx].min.x, 0.0);
        cursor = target_min + width;
    }

    offsets
}

/// Pack objects so their vertical edges abut, keeping the anchor object fixed.
pub fn move_together_v(bounds: &[Bounds], anchor_index: usize) -> Vec<Point2D> {
    if bounds.is_empty() || anchor_index >= bounds.len() {
        return vec![Point2D::zero(); bounds.len()];
    }

    let mut offsets = vec![Point2D::zero(); bounds.len()];
    let anchor = bounds[anchor_index];
    let anchor_center = (anchor.min.y + anchor.max.y) / 2.0;

    let mut above: Vec<usize> = (0..bounds.len())
        .filter(|&idx| {
            idx != anchor_index && (bounds[idx].min.y + bounds[idx].max.y) / 2.0 < anchor_center
        })
        .collect();
    above.sort_by(|&a, &b| {
        let ac = (bounds[a].min.y + bounds[a].max.y) / 2.0;
        let bc = (bounds[b].min.y + bounds[b].max.y) / 2.0;
        bc.partial_cmp(&ac).unwrap()
    });

    let mut cursor = anchor.min.y;
    for idx in above {
        let height = bounds[idx].max.y - bounds[idx].min.y;
        let target_min = cursor - height;
        offsets[idx] = Point2D::new(0.0, target_min - bounds[idx].min.y);
        cursor = target_min;
    }

    let mut below: Vec<usize> = (0..bounds.len())
        .filter(|&idx| {
            idx != anchor_index && (bounds[idx].min.y + bounds[idx].max.y) / 2.0 >= anchor_center
        })
        .collect();
    below.sort_by(|&a, &b| {
        let ac = (bounds[a].min.y + bounds[a].max.y) / 2.0;
        let bc = (bounds[b].min.y + bounds[b].max.y) / 2.0;
        ac.partial_cmp(&bc).unwrap()
    });

    cursor = anchor.max.y;
    for idx in below {
        let height = bounds[idx].max.y - bounds[idx].min.y;
        let target_min = cursor;
        offsets[idx] = Point2D::new(0.0, target_min - bounds[idx].min.y);
        cursor = target_min + height;
    }

    offsets
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bounds(x: f64, y: f64, w: f64, h: f64) -> Bounds {
        Bounds::new(Point2D::new(x, y), Point2D::new(x + w, y + h))
    }

    #[test]
    fn align_left_moves_to_leftmost() {
        let bounds = vec![
            make_bounds(10.0, 0.0, 20.0, 10.0),
            make_bounds(30.0, 0.0, 20.0, 10.0),
            make_bounds(50.0, 0.0, 20.0, 10.0),
        ];
        let offsets = align_left(&bounds);
        assert!((offsets[0].x - 0.0).abs() < 1e-10); // already leftmost
        assert!((offsets[1].x - (-20.0)).abs() < 1e-10);
        assert!((offsets[2].x - (-40.0)).abs() < 1e-10);
        // No vertical movement
        for o in &offsets {
            assert!((o.y - 0.0).abs() < 1e-10);
        }
    }

    #[test]
    fn align_right_moves_to_rightmost() {
        let bounds = vec![
            make_bounds(10.0, 0.0, 20.0, 10.0), // max.x = 30
            make_bounds(30.0, 0.0, 20.0, 10.0), // max.x = 50
            make_bounds(50.0, 0.0, 20.0, 10.0), // max.x = 70
        ];
        let offsets = align_right(&bounds);
        assert!((offsets[0].x - 40.0).abs() < 1e-10);
        assert!((offsets[1].x - 20.0).abs() < 1e-10);
        assert!((offsets[2].x - 0.0).abs() < 1e-10); // already rightmost
    }

    #[test]
    fn align_top_moves_to_topmost() {
        let bounds = vec![
            make_bounds(0.0, 10.0, 10.0, 20.0),
            make_bounds(0.0, 40.0, 10.0, 20.0),
            make_bounds(0.0, 70.0, 10.0, 20.0),
        ];
        let offsets = align_top(&bounds);
        assert!((offsets[0].y - 0.0).abs() < 1e-10);
        assert!((offsets[1].y - (-30.0)).abs() < 1e-10);
        assert!((offsets[2].y - (-60.0)).abs() < 1e-10);
        for o in &offsets {
            assert!((o.x - 0.0).abs() < 1e-10);
        }
    }

    #[test]
    fn align_bottom_moves_to_bottommost() {
        let bounds = vec![
            make_bounds(0.0, 10.0, 10.0, 20.0), // max.y = 30
            make_bounds(0.0, 40.0, 10.0, 20.0), // max.y = 60
            make_bounds(0.0, 70.0, 10.0, 20.0), // max.y = 90
        ];
        let offsets = align_bottom(&bounds);
        assert!((offsets[0].y - 60.0).abs() < 1e-10);
        assert!((offsets[1].y - 30.0).abs() < 1e-10);
        assert!((offsets[2].y - 0.0).abs() < 1e-10); // already bottommost
    }

    #[test]
    fn align_center_h_uses_average_center() {
        let bounds = vec![
            make_bounds(0.0, 0.0, 20.0, 10.0),  // center_x = 10
            make_bounds(40.0, 0.0, 20.0, 10.0), // center_x = 50
            make_bounds(80.0, 0.0, 20.0, 10.0), // center_x = 90
        ];
        // group center_x = (10 + 50 + 90) / 3 = 50
        let offsets = align_center_h(&bounds);
        assert!((offsets[0].x - 40.0).abs() < 1e-10); // 50 - 10
        assert!((offsets[1].x - 0.0).abs() < 1e-10); // 50 - 50
        assert!((offsets[2].x - (-40.0)).abs() < 1e-10); // 50 - 90
    }

    #[test]
    fn align_center_v_uses_average_center() {
        let bounds = vec![
            make_bounds(0.0, 0.0, 10.0, 20.0),  // center_y = 10
            make_bounds(0.0, 40.0, 10.0, 20.0), // center_y = 50
            make_bounds(0.0, 80.0, 10.0, 20.0), // center_y = 90
        ];
        // group center_y = (10 + 50 + 90) / 3 = 50
        let offsets = align_center_v(&bounds);
        assert!((offsets[0].y - 40.0).abs() < 1e-10);
        assert!((offsets[1].y - 0.0).abs() < 1e-10);
        assert!((offsets[2].y - (-40.0)).abs() < 1e-10);
    }

    #[test]
    fn align_to_anchor_uses_explicit_anchor() {
        let bounds = vec![
            make_bounds(0.0, 0.0, 10.0, 10.0),
            make_bounds(30.0, 5.0, 20.0, 10.0),
            make_bounds(80.0, 20.0, 10.0, 10.0),
        ];
        let offsets = align_to_anchor(&bounds, AnchorAlignment::Left, 1, &[true, true, true]);
        assert_eq!(offsets[1], Point2D::zero());
        assert!((offsets[0].x - 30.0).abs() < 1e-10);
        assert!((offsets[2].x - (-50.0)).abs() < 1e-10);
        assert_eq!(offsets[0].y, 0.0);
        assert_eq!(offsets[2].y, 0.0);
    }

    #[test]
    fn align_to_anchor_skips_immovable_entries() {
        let bounds = vec![
            make_bounds(0.0, 0.0, 10.0, 10.0),
            make_bounds(30.0, 0.0, 10.0, 10.0),
            make_bounds(60.0, 0.0, 10.0, 10.0),
        ];
        let offsets = align_to_anchor(&bounds, AnchorAlignment::Right, 2, &[true, false, true]);
        assert!((offsets[0].x - 60.0).abs() < 1e-10);
        assert_eq!(offsets[1], Point2D::zero());
        assert_eq!(offsets[2], Point2D::zero());
    }

    #[test]
    fn align_to_anchor_documents_center_axis_names() {
        let bounds = vec![
            make_bounds(0.0, 0.0, 10.0, 20.0),   // center = 5,10
            make_bounds(40.0, 50.0, 20.0, 10.0), // center = 50,55
        ];

        let vertical_centers =
            align_to_anchor(&bounds, AnchorAlignment::CentersV, 1, &[true, true]);
        assert!((vertical_centers[0].x - 45.0).abs() < 1e-10);
        assert_eq!(vertical_centers[0].y, 0.0);

        let horizontal_centers =
            align_to_anchor(&bounds, AnchorAlignment::CentersH, 1, &[true, true]);
        assert_eq!(horizontal_centers[0].x, 0.0);
        assert!((horizontal_centers[0].y - 45.0).abs() < 1e-10);

        let both_axes = align_to_anchor(&bounds, AnchorAlignment::CentersXy, 1, &[true, true]);
        assert!((both_axes[0].x - 45.0).abs() < 1e-10);
        assert!((both_axes[0].y - 45.0).abs() < 1e-10);
    }

    #[test]
    fn distribute_h_spaces_evenly() {
        let bounds = vec![
            make_bounds(0.0, 0.0, 10.0, 10.0),  // center_x = 5
            make_bounds(10.0, 0.0, 10.0, 10.0), // center_x = 15
            make_bounds(80.0, 0.0, 10.0, 10.0), // center_x = 85
        ];
        // sorted by center: 5, 15, 85 => target centers: 5, 45, 85
        let offsets = distribute_h_centered(&bounds);
        assert!((offsets[0].x - 0.0).abs() < 1e-10); // stays at 5
        assert!((offsets[1].x - 30.0).abs() < 1e-10); // 45 - 15
        assert!((offsets[2].x - 0.0).abs() < 1e-10); // stays at 85
    }

    #[test]
    fn distribute_v_spaces_evenly() {
        let bounds = vec![
            make_bounds(0.0, 0.0, 10.0, 10.0),  // center_y = 5
            make_bounds(0.0, 10.0, 10.0, 10.0), // center_y = 15
            make_bounds(0.0, 80.0, 10.0, 10.0), // center_y = 85
        ];
        // sorted by center: 5, 15, 85 => target centers: 5, 45, 85
        let offsets = distribute_v_centered(&bounds);
        assert!((offsets[0].y - 0.0).abs() < 1e-10);
        assert!((offsets[1].y - 30.0).abs() < 1e-10);
        assert!((offsets[2].y - 0.0).abs() < 1e-10);
    }

    #[test]
    fn distribute_h_spaced_equalizes_edge_gaps() {
        let bounds = vec![
            make_bounds(0.0, 0.0, 10.0, 10.0),
            make_bounds(20.0, 0.0, 20.0, 10.0),
            make_bounds(80.0, 0.0, 10.0, 10.0),
        ];
        // Outer span is 90, total width is 40, so both edge gaps become 25.
        let offsets = distribute_h_spaced(&bounds);
        assert_eq!(offsets[0], Point2D::zero());
        assert!((offsets[1].x - 15.0).abs() < 1e-10);
        assert_eq!(offsets[2], Point2D::zero());
    }

    #[test]
    fn distribute_v_spaced_equalizes_edge_gaps() {
        let bounds = vec![
            make_bounds(0.0, 0.0, 10.0, 10.0),
            make_bounds(0.0, 20.0, 10.0, 20.0),
            make_bounds(0.0, 80.0, 10.0, 10.0),
        ];
        // Outer span is 90, total height is 40, so both edge gaps become 25.
        let offsets = distribute_v_spaced(&bounds);
        assert_eq!(offsets[0], Point2D::zero());
        assert!((offsets[1].y - 15.0).abs() < 1e-10);
        assert_eq!(offsets[2], Point2D::zero());
    }

    #[test]
    fn distribute_h_with_fewer_than_3_returns_zero_offsets() {
        let bounds = vec![
            make_bounds(0.0, 0.0, 10.0, 10.0),
            make_bounds(50.0, 0.0, 10.0, 10.0),
        ];
        let offsets = distribute_h_centered(&bounds);
        assert_eq!(offsets.len(), 2);
        assert!((offsets[0].x).abs() < 1e-10);
        assert!((offsets[1].x).abs() < 1e-10);
    }

    #[test]
    fn distribute_v_with_fewer_than_3_returns_zero_offsets() {
        let bounds = vec![make_bounds(0.0, 0.0, 10.0, 10.0)];
        let offsets = distribute_v_centered(&bounds);
        assert_eq!(offsets.len(), 1);
        assert!((offsets[0].y).abs() < 1e-10);
    }

    #[test]
    fn align_left_empty_input() {
        let offsets = align_left(&[]);
        assert!(offsets.is_empty());
    }

    #[test]
    fn align_right_empty_input() {
        let offsets = align_right(&[]);
        assert!(offsets.is_empty());
    }

    #[test]
    fn align_single_object_returns_zero_offset() {
        let bounds = vec![make_bounds(10.0, 20.0, 30.0, 40.0)];
        let offsets = align_left(&bounds);
        assert_eq!(offsets.len(), 1);
        assert!((offsets[0].x).abs() < 1e-10);
        assert!((offsets[0].y).abs() < 1e-10);
    }

    #[test]
    fn distribute_h_preserves_order_with_unsorted_input() {
        // Objects given out-of-order by center_x
        let bounds = vec![
            make_bounds(80.0, 0.0, 10.0, 10.0), // center_x = 85, idx 0
            make_bounds(0.0, 0.0, 10.0, 10.0),  // center_x = 5, idx 1
            make_bounds(40.0, 0.0, 10.0, 10.0), // center_x = 45, idx 2
            make_bounds(20.0, 0.0, 10.0, 10.0), // center_x = 25, idx 3
        ];
        // sorted: idx1(5), idx3(25), idx2(45), idx0(85)
        // step = (85 - 5) / 3 = 80/3 ~= 26.667
        // targets: 5, 31.667, 58.333, 85
        let offsets = distribute_h_centered(&bounds);
        // idx0 (85) is last in sort -> stays at 85
        assert!((offsets[0].x - 0.0).abs() < 1e-10);
        // idx1 (5) is first in sort -> stays at 5
        assert!((offsets[1].x - 0.0).abs() < 1e-10);
        // idx2 (45) is rank 2 -> target 5 + 80/3*2 = 58.333, offset = 58.333 - 45 = 13.333
        assert!((offsets[2].x - (80.0 / 3.0 * 2.0 + 5.0 - 45.0)).abs() < 1e-10);
        // idx3 (25) is rank 1 -> target 5 + 80/3 = 31.667, offset = 31.667 - 25 = 6.667
        assert!((offsets[3].x - (80.0 / 3.0 + 5.0 - 25.0)).abs() < 1e-10);
    }

    #[test]
    fn distribute_h_four_objects_evenly() {
        let bounds = vec![
            make_bounds(0.0, 0.0, 10.0, 10.0),  // center_x = 5
            make_bounds(30.0, 0.0, 10.0, 10.0), // center_x = 35
            make_bounds(60.0, 0.0, 10.0, 10.0), // center_x = 65
            make_bounds(90.0, 0.0, 10.0, 10.0), // center_x = 95
        ];
        // Already evenly distributed with step = 30
        let offsets = distribute_h_centered(&bounds);
        for o in &offsets {
            assert!((o.x).abs() < 1e-10);
        }
    }

    #[test]
    fn distribute_h_nan_returns_zero_offsets() {
        let bounds = vec![
            make_bounds(f64::NAN, 0.0, 10.0, 10.0),
            make_bounds(10.0, 0.0, 10.0, 10.0),
            make_bounds(50.0, 0.0, 10.0, 10.0),
        ];
        let offsets = distribute_h_centered(&bounds);
        assert_eq!(offsets.len(), 3);
        for o in &offsets {
            assert!((o.x).abs() < 1e-10);
            assert!((o.y).abs() < 1e-10);
        }
    }

    #[test]
    fn distribute_h_infinity_returns_zero_offsets() {
        let bounds = vec![
            make_bounds(0.0, 0.0, 10.0, 10.0),
            make_bounds(f64::INFINITY, 0.0, 10.0, 10.0),
            make_bounds(50.0, 0.0, 10.0, 10.0),
        ];
        let offsets = distribute_h_centered(&bounds);
        assert_eq!(offsets.len(), 3);
        for o in &offsets {
            assert!((o.x).abs() < 1e-10);
            assert!((o.y).abs() < 1e-10);
        }
    }

    #[test]
    fn distribute_v_nan_returns_zero_offsets() {
        let bounds = vec![
            make_bounds(0.0, f64::NAN, 10.0, 10.0),
            make_bounds(0.0, 10.0, 10.0, 10.0),
            make_bounds(0.0, 50.0, 10.0, 10.0),
        ];
        let offsets = distribute_v_centered(&bounds);
        assert_eq!(offsets.len(), 3);
        for o in &offsets {
            assert!((o.x).abs() < 1e-10);
            assert!((o.y).abs() < 1e-10);
        }
    }

    #[test]
    fn distribute_v_infinity_returns_zero_offsets() {
        let bounds = vec![
            make_bounds(0.0, 0.0, 10.0, 10.0),
            make_bounds(0.0, f64::NEG_INFINITY, 10.0, 10.0),
            make_bounds(0.0, 50.0, 10.0, 10.0),
        ];
        let offsets = distribute_v_centered(&bounds);
        assert_eq!(offsets.len(), 3);
        for o in &offsets {
            assert!((o.x).abs() < 1e-10);
            assert!((o.y).abs() < 1e-10);
        }
    }

    #[test]
    fn move_together_h_keeps_anchor_fixed_and_abuts_edges() {
        let bounds = vec![
            make_bounds(0.0, 0.0, 10.0, 10.0),
            make_bounds(30.0, 0.0, 20.0, 10.0),
            make_bounds(80.0, 0.0, 10.0, 10.0),
        ];
        let offsets = move_together_h(&bounds, 1);
        assert_eq!(offsets[1], Point2D::zero());
        assert!((offsets[0].x - 20.0).abs() < 1e-10);
        assert!((offsets[2].x - (-30.0)).abs() < 1e-10);
        assert_eq!(offsets[0].y, 0.0);
        assert_eq!(offsets[2].y, 0.0);
    }

    #[test]
    fn move_together_v_keeps_anchor_fixed_and_abuts_edges() {
        let bounds = vec![
            make_bounds(0.0, 0.0, 10.0, 10.0),
            make_bounds(0.0, 30.0, 10.0, 20.0),
            make_bounds(0.0, 80.0, 10.0, 10.0),
        ];
        let offsets = move_together_v(&bounds, 1);
        assert_eq!(offsets[1], Point2D::zero());
        assert!((offsets[0].y - 20.0).abs() < 1e-10);
        assert!((offsets[2].y - (-30.0)).abs() < 1e-10);
        assert_eq!(offsets[0].x, 0.0);
        assert_eq!(offsets[2].x, 0.0);
    }
}
